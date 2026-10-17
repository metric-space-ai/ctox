import {createPreferencesApi,preferenceError,PREFERENCE_COLLECTION} from './preferences.mjs';
import {PLATFORM_MANIFEST} from './platform-manifest.generated.mjs';
import {commandTarget} from './command-routing.mjs';
import { createCommandTimings } from './command-timings.mjs';
import { PLATFORM_COLLECTIONS, PLATFORM_APP_ID } from './platform.mjs';
import { artifactUrl, utf8Base64, bytesBase64 } from './artifact-url.mjs';
import { takeoverV3Database } from './migrate-v3.mjs';
import { snapshotBytes } from './snapshot.mjs';
import { evictCollection, reclaimCache, CLIENT_CACHE_BUDGET } from './cache.mjs';
import { KernelTransport, decimalSeq, seqGreater, maxSeq, checkpointKey } from './transport.mjs';
import { assertLeader, acquireGeneration } from './leader.mjs';
// Local App API 4.0 kernel. Derived from the reviewed executable contract; no legacy runtime imports.
import { CtoxError, StaleLeader, stripReadMetadata, cloneValue, deepFreeze, deepEqual, jsonEq, liveEqual, mergePatch, fnv1a, uuidv7, sha256Js, sha256Hex, hasBlob, isBlob, isPlainObject } from './values.mjs';
import { compileQuery, runCompiledQuery, normalizeJson, validateSchema } from './app-api.mjs';
import { openBackend, deleteBackend } from './store-idb-v5.mjs';
import { emptyBase, byOpId, enqueueWrite, reserveWrites, docKey, nsName, applyOp, recompute } from './outbox.mjs';
import { checkCommandTransition } from './command-state.mjs';
import { SCHEMA_FILE, parseCollectionsFile, classifyCollections, applyMigrationSteps, manifestCollectionsToParsed, parsedToManifestCollections } from './schema-diff.mjs';
export { CtoxError, uuidv7, sha256Hex, sha256Js } from './values.mjs';
export { compileQuery, runCompiledQuery } from './app-api.mjs';
export { SCHEMA_FILE, parseCollectionsFile, classifyCollections, applyMigrationSteps } from './schema-diff.mjs';
export { createShellKernel } from './shell-api.mjs';
export const APP_API_VERSION = '4.0';
const SNAPSHOT_MAX_BYTES = 1024 * 1024;
const SYSTEM_COLLECTIONS = new Set(PLATFORM_COLLECTIONS);
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
  // §16.22/§16.26/§16.39 Nr. 4: Entwurf je (App, Nutzer), Dateien als sha256 (Inhalt im CAS). Abweichung vom Plattform-
  // Manifest: files-Werte dürfen null sein (= im Entwurf gelöscht); das Manifest erlaubt nur Hashes (Befund K-1c).
  app_drafts: deepFreeze({
    primaryKey: 'id', type: 'object', additionalProperties: false, required: ['id', 'app_id', 'user_id'], indexes: ['app_id', 'user_id'],
    properties: {
      id: { type: 'string', maxLength: 180 }, app_id: { type: 'string' }, user_id: { type: 'string' },
      base_app_ver: { type: ['string', 'null'] }, manifest_fields: { type: 'object', additionalProperties: true },
      files: { type: 'object', additionalProperties: { type: ['string', 'null'] } },
      collections_schema: { type: 'object', additionalProperties: true }, updated_at_ms: { type: 'number' },
    },
  }),
});
const PLATFORM_APP = Object.freeze({ id: PLATFORM_APP_ID, ver: '1' });
const OWN_ROW_COLLECTIONS = new Set(['shell_preferences', 'app_drafts']);
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
const LIB_PREFIX = 'lib:';
const isLib = (appId) => typeof appId === 'string' && appId.startsWith(LIB_PREFIX);
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
const HASH_MISMATCH = Symbol('hash_mismatch');
export function createKernel(opts = {}) {
  const instanceId = opts.instanceId;
  const userId = opts.userId ?? opts.actor?.id;
  if(typeof instanceId !== 'string' || !instanceId || typeof userId !== 'string' || !userId)throw new CtoxError('invalid','Authenticated instanceId and userId required');
  if(opts.actor?.id && opts.actor.id !== userId)throw new CtoxError('forbidden','Actor identity mismatch');
  const dbName = opts.dbName ?? `ctox-${instanceId}-${userId}`; // §16.7: DB je (Instanz, Nutzer)
  const latencyMs = opts.latencyMs ?? 50;
  const transport = opts.transport ?? new KernelTransport();
  const fakeCore = transport.kind === 'memory' ? transport : null;
  const subscriptions = new Set();
  const scopeCursors = new Map();
  const expectedScopes = new Map();
  let timing = null;
  const scopeBacklog = new Map();
  const evicted = new Set();
  const lastUsed = new Map();
  let transportConnected = false;
  const remountTimeoutMs = opts.shellStartTimeoutMs ?? 9000;
  const generationGraceMs = opts.generationGraceMs ?? 600000;
  const getTabKey = () => opts.tabKey ?? (()=>{try{const key=`ctox-tab:${dbName}`;let id=sessionStorage.getItem(key);if(!id){id=uuidv7();sessionStorage.setItem(key,id);}return id;}catch{return tabId;}})();
  // §16.37/§16.39 Nr. 2: Byte-Budget der bei Bedarf geholten Bibliotheksdateien (LRU über IDB artifacts)
  const libByteBudget = opts.libByteBudget ?? 512 * 1024 * 1024;
  const capabilities = new Set(opts.capabilities ?? []);
  // §16.43: die device_pairing_id kommt aus der bestehenden Gerätebindung (Bootstrap, dieselbe id wie im hello device_proof);
  // der Fake-Core liefert sie deterministisch je (Instanz, Nutzer, Gerät).
  const deviceId = () => opts.deviceId ?? transport.deviceId?.(instanceId, userId) ?? null;
  const tabId = uuidv7();
  const tabKey = getTabKey();
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
  const cmdSubmitFailed = new Map();
  const zone = [];
  const moduleCache = new Map();
  const routeListeners = new Map();
  const fileReadGrants = new Map(); // E48: session-local, exact ref, consumed once, one hour.

  const windowStates = new Map();
  let locale = opts.locale ?? 'de';
  const translations = {};
  let theme = { ...(opts.theme ?? { '--ctox-bg': '#ffffff', '--ctox-fg': '#1d1d1f', '--ctox-accent': '#2f6fed' }) };
  let actor = { id: userId, display_name: 'Lokaler Nutzer', role: '', permissions: [], ...(opts.actor ?? {}) };
  const timers = new Set(), tasks = new Set(), deadlineCancels = new Set();
  const inflight = new Map(); // docKey -> op_id
  const inflightOps = new Set();
  const reservedOps = new Set();
  const skipBeforeCommit = new Set();
  const uploadProgress = new Map(), uploadsDone = new Set(), uploadResults = new Map();
  const activationListeners = new Set();
  let activationReports=Promise.resolve();
  const activationChains = new Map();
  let corruptNextFetch=0;
  const publishing = new Set();
  const doubleWrite = new Map(); // ns_old -> {ns_new, migrate}
  const sim0 = { reject: [], migrate: [], failCmd: new Map(), cmdHandlers: new Map() };
  const simRecord = { lastVisible: null, activationReports: [], navigations: [], dialogs: [], openedFiles: [], staleDiscards: 0, refetches:0 };
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
    // Catalog changes can move an already open view to a new generation.
    const depend = tag => {
      if(tags.includes(tag))return;tags.push(tag);
      if(subs.size){if(!tagIndex.has(tag))tagIndex.set(tag,new Set());tagIndex.get(tag).add(impl);}
    };
    let last, has = false;
    const impl = {
      refresh() {
        if (!subs.size) return;
        const v = deepFreeze(compute(depend));
        if (has && liveEqual(last, v)) return;
        last = v; has = true;
        for (const cb of [...subs]) { try { cb(v); } catch (e) { console.error('[ctox-kernel] subscriber', e); } }
      },
      dispose() { subs.clear(); for (const t of tags) tagIndex.get(t)?.delete(impl); },
    };
    return {
      get: () => { guardAlive(); return deepFreeze(compute(depend)); },
      subscribe(cb) {
        if (typeof cb !== 'function') throw new CtoxError('invalid', 'subscribe braucht eine Funktion');
        if (closed || loggedOut) return () => {};
        if (!subs.size) { for (const t of tags) { if (!tagIndex.has(t)) tagIndex.set(t, new Set()); tagIndex.get(t).add(impl); } allLives.add(impl); }
        subs.add(cb);
        last = deepFreeze(compute(depend)); has = true;
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
  function readDocument(rec, data = visible(rec)) { return data == null ? null : {...cloneValue(data), _rev:rec?.base?.rev ?? null, _pending:!!rec?.pending}; }
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
    if (rec.collection === 'business_commands') {
      if (rec.pending && rec.lseq) commandTimings.mark(rec.id, 'queued');
      if (!rec.pending && ['done','failed','cancelled','rejected'].includes(r.data?.status)) commandTimings.mark(rec.id, 'completed');
      notify('commands');
    }
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
    for(const collection of result.collections){evicted.add(collection);scopeCursors.delete(collection);subscriptions.delete(collection);untrackCollection(collection);}
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
  const commandTimings = createCommandTimings((value, broadcast, patch) => {
    notify('metrics');
    if (broadcast) post({type:'command_timing', value:patch});
    try {
      const observed = opts.diagnostics?.onCommandTiming?.(value);
      if (observed != null) Promise.resolve(observed).catch(() => {});
    } catch { /* diagnostics cannot affect writes */ }
  });
  async function refreshOutboxStat() {
    if (!db || closed) return;
    const next = await db.outboxStat();
    if (next.count !== outboxStat.count || next.oldest_at !== outboxStat.oldest_at) { outboxStat = next; notify('metrics'); }
  }
  function netChanged() {
    notify('metrics');
    if (isLeader) {
      refreshCollections();
      const state={link:{...link},backlog:(fakeCore?.backlog ?? 0)+extraBacklog,collections:collectionsView};
      post({type:'net',...state});
      if(db && !closed)track(db.tx(async t=>{
        await assertGen(t);const old=await t.get('meta','network');
        if(jsonEq(old?.value,state))return;
        await t.put('meta',{key:'network',value:state});await bumpLocalSeq(t);
      }).catch(err=>{if(err instanceof StaleLeader)stepDown();else throw err;}));
    }
  }
  // ---- §16.45 Collection-Liveness (K-1e): je abonnierter Collection {state, backlog, gen, since_ms, error?} ----
  // live = Verbindung oben UND Backlog 0 nach vollständiger Seite für den abonnierten Bereich (collection, scope, gen);
  // catching_up = noch keine vollständige Seite oder Backlog > 0; offline = Verbindung weg; error = typisierter Fehler (§16.42).
  const collScopes = new Map(); // collection → {key, gen, backlog, complete, error, state, since_ms}
  let collectionsView = {};
  function collState(e) {
    if (e.error) return 'error';
    if (link.state !== 'online') return 'offline';
    return e.complete && e.backlog === 0 ? 'live' : 'catching_up';
  }
  function refreshCollections() {
    const now = Date.now(), view = {};
    for (const [name, e] of collScopes) {
      const st = collState(e);
      if (st !== e.state) { e.state = st; e.since_ms = now; }
      view[name] = { state: e.state, backlog: e.backlog, gen: e.gen, since_ms: e.since_ms, ...(e.error ? { error: { ...e.error } } : {}) };
    }
    if (!jsonEq(view, collectionsView)) { collectionsView = view; return true; }
    return false;
  }
  // report=false: der Aufrufer meldet selbst, nachdem auch Checkpoint und Rückstand nachgezogen sind (K-BF1).
  function trackScope(collection, scope, patch, report = true) {
    const key = checkpointKey(scope);
    let e = collScopes.get(collection);
    if (!e || e.key !== key) { e = { key, gen: scope.gen ?? genOf(collection), backlog: 0, complete: false, error: null, state: null, since_ms: Date.now() }; collScopes.set(collection, e); }
    Object.assign(e, patch);
    if (refreshCollections() && report) netChanged();
  }
  function untrackCollection(collection) { if (collScopes.delete(collection) && refreshCollections()) netChanged(); }
  let followerNet = null;
  const metricsLive = makeLive(['metrics'], () => {
    const net = isLeader || !followerNet ? { link, backlog: (fakeCore?.backlog ?? 0) + extraBacklog, collections: collectionsView } : followerNet;
    return {
      commands: commandTimings.snapshot(),
      backlog: net.backlog,
      collections: cloneValue(net.collections ?? {}),
      outbox: { count: outboxStat.count, oldest_ms: outboxStat.oldest_at ? Math.max(0, Date.now() - outboxStat.oldest_at) : 0 },
      link: { ...net.link }, ...(timing?{timing:{...timing}}:{}),
    };
  });
  Object.defineProperty(metricsLive, 'commands', {get: () => { guardAlive(); return deepFreeze(commandTimings.snapshot()); }});
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
  const VERSION_CODES = new Set(['app.needs_update', 'app.activation_failed', 'app.remount_failed']);
  function clearStaleVersionEntries(appId, ver) {
    const before = zone.length;
    for (let i = zone.length - 1; i >= 0; i--) {
      const e = zone[i];
      if (e.app_id === appId && e.source === 'kernel' && VERSION_CODES.has(e.code) && e.ref !== ver) zone.splice(i, 1);
    }
    if (zone.length !== before) notify('status');
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
    report:(appId,item)=>zoneReport(appId,item,'shell'),
  };

  // ---- Laden, Leiterwahl ----
  async function loadFromStorage() {
    for (const e of await db.all('outbox')) if (e.client_seq != null) reservedOps.add(e.op_id);
    const docs = await db.all('docs');
    for (const r of docs) mirrorPut(r);
    const metas = await db.all('meta');
    for (const m of metas) {
      if(m.key.startsWith('upload_done:'))uploadsDone.add(m.ref);
      if (m.key.startsWith('upload_result:')) uploadResults.set(m.op_id,m);
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
      if(meta.key.startsWith('upload_result:')){uploadResults.set(meta.op_id,meta);notify('uploads');}
      if(meta.key.startsWith('upload_done:')){uploadsDone.add(meta.ref);notify('uploads');}
      if(meta.key.startsWith('retired:') && (meta.record?.lseq??0)>known)mirrorPut(meta.record);
      if(meta.key==='checkpoint')setCheckpoint(meta.value);
      if(meta.key==='network' && !isLeader){followerNet=meta.value;notify('metrics');}
      if(meta.key.startsWith('activation:'))applyActivationLocal(meta,false);
      if(meta.key.startsWith('result:') && waiters.has(meta.info.op_id))handleWriteResult(meta.info,meta.result);
    }
    localSeqKnown=ls;await refreshOutboxStat();if(isLeader)schedulePump();return n;
  }
  // Verbindungsaufbau (Leiter): connect, dann alle Bereiche abonnieren. Auch der Weg von kernel.reconnect().
  function connectTransport() {
    return Promise.resolve().then(()=>transport.connect({instanceId,userId},{
        page:receivePage,write_result:receiveResult,
        link:next=>{link={...link,...next,state:next.state==='online'?'online':'offline'};if(link.state==='offline'){inflight.clear();inflightOps.clear();}netChanged();if(link.state==='online'){for(const value of activations.values())sendActivationReport(value);for(const id of mirror.get('business_apps')?.keys()??[])scheduleActivation(id);if(transport.scope)for(const collection of subscriptions)track(subscribeCollection(collection));schedulePump();}},
        actor:next=>{if(next.id!==userId)throw new CtoxError('forbidden','Actor identity mismatch');actor={...actor,...next};notify('actor');},
        timing:next=>{timing=next;notify('metrics');},
        status:(appId,item)=>zoneReport(appId,item,'kernel'),
        revoke:receiveRevoke,
      })).then(async()=>{
        if(closed||!isLeader)return;
        for(const collection of new Set(['business_apps','business_app_versions','business_commands','files','shell_preferences','app_user_preferences','app_drafts',...mirror.keys()]))await subscribeCollection(collection);
        await deliver();schedulePump();
      }).catch(error=>{if(!closed){link={...link,state:'offline'};netChanged();zoneReport('kernel',{level:'error',code:'transport.connect',text:error.message},'kernel');}});
  }
  // §16.50 Nr. 3: Leitermeldung an den SW – beim Start, bei jedem Leiterwechsel (Erwerb und Verlust) und nach
  // controllerchange. Nur ein Hinweis für das Routing von Lib-Misses, keine Autorität (die Generation bleibt im IDB).
  // Ohne navigator.serviceWorker (Node, Worker) oder ohne controller ein No-op.
  const swContainer = (() => { try { return globalThis.navigator?.serviceWorker ?? null; } catch { return null; } })();
  let swHintStarted = false, swHintLast = null;
  function sendLeaderHint(force = false) {
    if (!swContainer || (!force && (!swHintStarted || swHintLast === isLeader))) return;
    let controller = null;
    try { controller = swContainer.controller; } catch { /* kein SW */ }
    if (!controller) return;
    try { controller.postMessage({ type: 'ctox:kernel', leader: isLeader }); swHintLast = isLeader; } catch { /* SW weg */ }
  }
  const onControllerChange = () => { if (!closed && swHintStarted) sendLeaderHint(true); };
  try { swContainer?.addEventListener?.('controllerchange', onControllerChange); } catch { /* kein SW */ }
  async function becomeLeader() {
    if (closed) return;
    myGen = await acquireGeneration(db);
    isLeader = true;
    for (const e of await db.all('outbox')) if (e.client_seq != null) reservedOps.add(e.op_id);
    sendLeaderHint();
    // Ein neuer Leiter hat alles gelesen, was der alte geschrieben hat (gemeinsame IDB).
    for (const r of await db.all('docs')) mirrorPut(r);
    for (const m of await db.all('meta')) {
      if (m.key.startsWith('activation:')) applyActivationLocal(m, false);
      // Takeover first advances the document mirror. Hydrate receipts here as
      // well; local_seq may already equal the checkpoint after that advance.
      if (m.key.startsWith('upload_result:')) { uploadResults.set(m.op_id,m);notify('uploads'); }
      if (m.key.startsWith('result:') && waiters.has(m.info.op_id)) handleWriteResult(m.info,m.result);
    }
    await settleAwaitingPages();
    notify('leader');
    post({ type: 'leader', gen: myGen });
    if (!transportConnected) {
      transportConnected = true;
      track(connectTransport());
    }
    netChanged();scheduleCacheBudget();schedulePump();
    for (const id of mirror.get('business_apps')?.keys() ?? []) scheduleActivation(id);
  }
  function stepDown() {
    if (!isLeader) return;
    simRecord.staleDiscards++;
    isLeader = false;
    for (const op of inflightOps) inflightOps.delete(op);
    inflight.clear();
    sendLeaderHint();
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
      case 'command_timing': commandTimings.merge(msg.value); break;
      case 'outbox': await refreshOutboxStat(); if (isLeader) schedulePump(); break;
      case 'priority': if(isLeader)transport.priority?.({app_id:msg.app_id,collections:msg.collections});break;
      case 'subscribe': if(isLeader && typeof msg.collection==='string')await subscribeCollection(msg.collection,'P1');break;
      case 'upload_result': uploadResults.set(msg.outcome.op_id,msg.outcome);notify('uploads');break;
      case 'upload_done': uploadsDone.add(msg.ref);notify('uploads');break;
      case 'write_result': handleWriteResult(msg.info, msg.result); await refreshOutboxStat(); break;
      case 'net': followerNet = { link: msg.link, backlog: msg.backlog, collections: msg.collections ?? {} }; notify('metrics'); break;
      case 'hello': if (isLeader) netChanged(); break;
      case 'leader': knownGen = Math.max(knownGen, msg.gen ?? 0); notify('leader'); break;
      case 'activation': applyActivationLocal(msg.value, false); break;
      case 'actor': actor = msg.actor; notify('actor'); break;
      case 'upload': setUploadProgress(msg.ref, msg.sent, msg.total); break;
      case 'rpc': {
        if (!isLeader) break;
        try {
          const value = msg.name==='fetchBlob' ? await fetchBlob(...msg.args) : msg.name==='fetchRange' ? await fetchRange(...msg.args) : await simImpl[msg.name](...msg.args);
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
  function localWrite(app, op, collection, id, body, createOnly = false, options = {}) {
    const op_id = uuidv7();
    let resolveResult;
    const result = new Promise((r) => { resolveResult = r; });
    let entry = null;
    let reused = false;
    let ownCommandQueued = false;
    // Synchroner Teil: prüfen und optimistisch in den Spiegel (lokal sofort sichtbar, Flag pending).
    const prepare = () => {
      guardAlive();
      if (options.expected_revision !== undefined && (typeof options.expected_revision !== 'string' || !options.expected_revision)) throw new CtoxError('invalid','expected_revision must be a nonempty revision',{path:'expected_revision'});
      if (collection === PREFERENCE_COLLECTION && options.preference !== true) throw new CtoxError('forbidden','Use api.prefs',{path:'collection'});
      if (typeof collection !== 'string' || !collection) throw new CtoxError('invalid', 'collection fehlt', { path: 'collection' });
      if(collection==='shell_preferences' && id!==userId)throw new CtoxError('forbidden','Only own shell preferences are writable',{path:'id'});
      if (typeof id !== 'string' || !id) throw new CtoxError('invalid', 'id (string) fehlt', { path: 'id' });
      if (!ownRowOk(collection, id)) throw new CtoxError('forbidden', `${collection}: nur die eigene Zeile (id = actor.id)`, { path: 'id' });
      if (collection === 'app_drafts' && app.id !== PLATFORM_APP.id) throw new CtoxError('forbidden', 'app_drafts nur über kernel.source(appId).draft.*', { path: 'collection' });
      const ns = resolveNs(app, collection);
      body = stripReadMetadata(body);
      if (isPlainObject(body) && Object.keys(body).some(key=>key.startsWith('_'))) throw new CtoxError('invalid','Underscore document fields are reserved',{path:Object.keys(body).find(key=>key.startsWith('_'))});
      let clean = null;
      if (op === 'put') {
        clean = normalizeJson(body, '');
        if (!isPlainObject(clean)) throw new CtoxError('invalid', 'Dokument muss ein Objekt sein', { path: '' });
      } else if (op === 'patch') clean = normalizeJsonPatch(body);
      const cur = mirrorGet(ns, id);
      const st = applyOp(cur ? { data: cur.data, deleted: cur.deleted } : { data: null, deleted: false }, { op, body: clean, id });
      if (collection===PREFERENCE_COLLECTION) {
        const rows=[...(mirror.get(ns)?.values()??[])].map(visible).filter(Boolean);
        const error=preferenceError(app.id,userId,id,st.deleted?null:st.data,rows);
        if(error)throw new CtoxError(error,'Preference rejected',{path:'key'});
      }
      if (st.data) validateSchema(st.data, schemaFor(app, collection), ''); // lokale Schemaprüfung am Ergebnisdokument
      entry = {
        op_id, prev_op_id: cur?.pending && cur.rev?.startsWith('op:') ? cur.rev.slice(3) : null,
        collection: ns, id, op, body: clean, base_rev: cur?.rev ?? null, base_seq: decimalSeq(cur?.base?.seq ?? '0'),
        gen: genOf(ns), ...writeVersion(app, collection), app_id: app.id, created_at: Date.now(), origin_tab: tabId,
      };
      if (options.expected_revision !== undefined) entry.expected_revision = options.expected_revision;
      if (options.file_upload) { entry.file_upload = true; entry.file_upload_op = options.file_upload_op; }
      waiters.set(op_id, resolveResult);
      const base = cur?.base ?? emptyBase();
      if (!createOnly || !visible(cur)) mirrorPut({ collection: ns, id, data: st.data, deleted: st.deleted, rev: `op:${op_id}`, seq: base.seq, pending: true, base });
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
        if (ns === PREFERENCE_COLLECTION) {
          const rows=(await t.all('docs')).filter(r=>r.collection===ns&&!r.deleted&&r.data).map(r=>r.data);
          const error=preferenceError(app.id,userId,id,entry.body,rows);
          if(error)throw new CtoxError(error,'Preference rejected',{path:'key'});
        }
        // E26: the IDB transaction arbitrates concurrent tabs and reloads.
        // Reuse the original operation/result, never enqueue another command.
        if (createOnly && visible(stored)) {
          reused = true;
          waiters.delete(op_id);
          const pending = (await t.outboxFor(ns, id)).sort(byOpId).at(-1);
          if (pending) {
            const previous = waiters.get(pending.op_id);
            waiters.set(pending.op_id, (value) => { previous?.(value); resolveResult(value); });
          } else resolveResult({ status: 'accepted', rev: stored.rev, seq: stored.seq });
          return stored;
        }
        await enqueueWrite(t, entry); // preserve E26 reuse plus durable enqueue/client order
        const r = recompute(ns, id, stored?.base ?? emptyBase(), await t.outboxFor(ns, id));
        // Preserve the complete locally acknowledged version, not merely a patch.
        entry.local_version={data:cloneValue(r.data),deleted:r.deleted};
        await t.put('outbox',entry);
        await putDoc(t, r);
        return r;
      });
      ownCommandQueued = !reused;
      if (ns === 'business_commands' && rec.pending) commandTimings.mark(id, 'queued');
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
    return handleFor(op_id, localDone, result, () => reused || (createOnly && !ownCommandQueued) ? false : cancelWrite(op_id, () => entry));
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
    // Only the leader can synchronously prove that no durable dispatch was reserved.
    const canCancel = isLeader && !pumping && !inflightOps.has(op_id) && !reservedOps.has(op_id);
    if (!canCancel) return false;
    track((async () => {
      const rec = await db.tx(async (t) => {
        const queued = await t.get('outbox', op_id);
        if (!queued || queued.sent_gen || queued.client_seq != null) return null;
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
    if (info.collection === 'business_commands') {
      commandTimings.mark(info.id, 'acked');
      if (result.status === 'rejected') commandTimings.mark(info.id, 'completed');
    }
    const res = waiters.get(info.op_id);
    if (res) { waiters.delete(info.op_id); res(cloneValue(result)); }
    if (result.local_archived_ref && result.status !== 'rejected') zoneReport(info.app_id,{level:'warn',code:`write.reconciled:${info.op_id}`,text:'Lokale Version wurde vom Kern geändert; Eingabe im lokalen Journal gesichert',ref:result.local_archived_ref},'kernel');
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
        // E47/#905: push progress is the durable outbox/client_seq, independent of every pull cursor.
        const entries = await db.tx(async t => { await assertGen(t); return reserveWrites(t); });
        for (const e of entries) if (e.client_seq != null) reservedOps.add(e.op_id);
        // P8's bounded gap buffer holds 64 positions. A blocked document head
        // must not let thousands of later commands exhaust it after reconnect.
        const floor = entries.find(e => e.client_seq != null)?.client_seq;
        const first = new Map();
        for (const e of entries) { const k = docKey(e.collection, e.id); if (!first.has(k)) first.set(k, e); }
        for (const [k, e] of first) {
          if (inflight.has(k) || !isLeader || link.state !== 'online') continue;
          if (e.awaiting_page) continue;
          if (e.client_seq != null && BigInt(e.client_seq) >= BigInt(floor) + 64n) continue;
          inflight.set(k, e.op_id);
          inflightOps.add(e.op_id);
          if (e.op === 'upload') setUploadProgress(e.id, Math.floor((uploadProgress.get(e.id)?.total ?? 0) / 2), uploadProgress.get(e.id)?.total ?? 0, true);
          later(() => track(confirm(k, e)), latencyMs);
        }
      } while (pumpAgain);
    } catch (err) { if (err instanceof StaleLeader) stepDown(); else throw err; }
    finally { pumping = false; }
  }
  async function subscribeCollection(collection,priority='P2') {
    if (!isLeader || closed || !transportConnected) return;
    evicted.delete(collection);
    let scope;
    try {scope = transport.scope ? await transport.scope(collection) : {epoch:opts.epoch??'1',collection,scope_hash:opts.scopeHash??'local',gen:genOf(collection)};}
    catch(error){
      if(!transport.grantApp || error?.code!=='forbidden')throw error;
      const base=collection.replace(/@g\d+$/,''),owner=SYSTEM_COLLECTIONS.has(base)?PLATFORM_APP_ID:[...mirror.get('business_apps')?.values()??[]].map(visible).find(d=>versionDoc(d?.id,d?.active_ver)?.manifest?.collections?.[base])?.id;
      if(!owner)throw error;await transport.grantApp(owner);scope=await transport.scope(collection);
    }
    expectedScopes.set(collection,scope);
    const cp = await db.get('meta', checkpointKey(scope));
    scopeCursors.set(collection,cp?.cursor_to??'0');
    trackScope(collection, scope, { complete: false, error: null });
    try { await transport.subscribe({ ...scope, checkpoint_seq: cp?.cursor_to ?? '0', priority }); }
    catch (err) { trackScope(collection, scope, { error: { code: err?.code ?? 'unavailable', message: String(err?.message ?? err) } }); throw err; }
    subscriptions.add(collection);
  }
  async function receivePage(page) {
    if (!isLeader || closed) return;
    const scope = { ...page, epoch: page.epoch ?? opts.epoch ?? '1', scope_hash: page.scope_hash ?? opts.scopeHash ?? 'local', gen: page.gen ?? genOf(page.collection) };
    const expected=expectedScopes.get(page.collection)??{scope_hash:opts.scopeHash??'local',epoch:opts.epoch??'1'};
    if(scope.scope_hash !== expected.scope_hash || String(scope.epoch) !== String(expected.epoch)) throw new CtoxError('revoked', 'Unexpected scope or epoch');
    if(!Number.isSafeInteger(scope.gen) || scope.gen!==(expected.gen??genOf(page.collection)))throw new CtoxError('invalid','Page generation does not match collection namespace');
    const cursor = decimalSeq(page.cursor_to ?? page.seq_to ?? page.head_seq ?? '0');
    const out = await db.tx(async (t) => {
      await assertGen(t);
      const key = checkpointKey(scope), previous = await t.get('meta', key);
      const recs = [], refused = [];
      for (const change of page.docs ?? []) {
        const ns = change.collection ?? page.collection;
        if(ns !== page.collection) throw new CtoxError('invalid', 'Page mixes collections');
        const seq = decimalSeq(change.seq);
        if(seqGreater(seq,cursor)) throw new CtoxError('invalid', 'Document exceeds page cursor');
        const old = await t.get('docs', [ns, change.id]);
        if(!old || seqGreater(seq,old.base?.seq ?? '0')) {
          const deleted = change.deleted || change.op === 'delete' || change.op === 'unproject';
          // K-1c Punkt 10: Befehlsdokumente nur in erlaubter Richtung; ein beendeter Befehl bleibt beendet (genau einmal)
          if (ns === 'business_commands' && !deleted && old?.base?.data) {
            const bad = checkCommandTransition(old.base.data, change.data ?? change.doc ?? change.body);
            if (bad) { refused.push({ id: change.id, ...bad }); continue; }
          }
          const base = {data: deleted ? null : cloneValue(change.data ?? change.doc ?? change.body),deleted:!!deleted,seq,rev:change.rev,gen:scope.gen};
          const rec = recompute(ns,change.id,base,await t.outboxFor(ns,change.id));
          await putDoc(t,rec); recs.push(rec);
        }
      }
      await t.put('meta',{key,...scope,cursor_to:maxSeq(previous?.cursor_to ?? '0',cursor),complete:seqGreater(previous?.cursor_to??'0',cursor)?previous.complete:!!page.complete});
      const cp=maxSeq((await t.get('meta','checkpoint'))?.value ?? '0',cursor);
      await t.put('meta',{key:'checkpoint',value:cp});
      // A receipt is not a document. Only the normal, scope-checked page can
      // release a merged/migrated optimistic operation, including after restart.
      const settled=[];
      for(const entry of await t.all('outbox')) {
        if(entry.collection!==page.collection || !entry.pending_result)continue;
        const row=await t.get('docs',[entry.collection,entry.id]);
        if(!canonicalReady(row?.base,entry.pending_result))continue;
        settled.push(await settleWrite(t,entry,row.base,entry.pending_result));
      }
      return {recs,cp,refused,settled};
    }).catch(err => { if(err instanceof StaleLeader){stepDown();return null;} throw err; });
    if(!out)return;
    for (const r of out.refused) zoneReport('kernel', { level: 'warn', code: `command.${r.code}`, text: `Befehl ${r.id}: ${r.message} – Zustellung verworfen`, ref: r.id }, 'kernel');
    for(const rec of out.recs)mirrorPut(rec);
    for(const completed of out.settled)publishWrite(completed);
    if(out.settled.length)await refreshOutboxStat();
    scopeCursors.set(page.collection,maxSeq(scopeCursors.get(page.collection)??'0',cursor));notify('checkpoint');
    // K-BF1: erst Checkpoint, Rückstand und Collection-Zustand vollständig nachziehen, dann genau EINE Meldung an die
    // Folger. Vorher meldete trackScope() die neuen Collection-Zustände mit dem alten extraBacklog („alle live, backlog N“).
    scopeBacklog.set(checkpointKey(scope),Math.max(0,Number(page.backlog??0)));
    setCheckpoint(out.cp); extraBacklog=[...scopeBacklog.values()].reduce((a,b)=>a+b,0);
    trackScope(page.collection, scope, { backlog: Math.max(0, Number(page.backlog ?? 0)), complete: !!page.complete, error: null }, false);
    netChanged();
    post({type:'docs',keys:out.recs.map(r=>[r.collection,r.id]),checkpoint:out.cp});
    await settleAwaitingPages(page.collection);
  }
  async function settleAwaitingPages(collection) {
    let touched=false;
    for (const entry of await db.all('outbox')) if (entry.awaiting_page && (!collection || entry.collection===collection)) {await applyResult(entry,entry.awaiting_page);touched=true;}
    if(touched){await refreshOutboxStat();schedulePump();}
  }
  async function receiveResult(message) {
    const entry=await db.get('outbox',message.op_id);
    if(entry && message.client_seq!==undefined && message.client_seq!==entry.client_seq)throw new CtoxError('invalid','write_result client_seq mismatch');
    if(entry){await applyResult(entry,message);await refreshOutboxStat();schedulePump();}
  }
  // Durable CAS outcomes are per upload operation, not just per content hash.
  // This separates a new upload from an earlier accepted/rejected same-ref attempt.
  async function storeUploadResult(t, entry, result) {
    const outcome={key:`upload_result:${entry.op_id}`,op_id:entry.op_id,ref:entry.id,...result};
    await t.put('meta',outcome);
    let rec=null;
    if (result.status === 'rejected' && entry.file_id) {
      const row=await t.get('docs',['files',entry.file_id]);
      if (row?.data?.ref === entry.id && row.data.owner_app === entry.app_id) {
        rec={...row,data:{...row.data,state:'rejected'},deleted:false,local_file_rejection:true};
        await putDoc(t,rec);
      }
    }
    return {outcome,rec};
  }
  function publishUploadResult(value) {
    uploadResults.set(value.outcome.op_id,value.outcome);notify('uploads');
    if(value.rec){mirrorPut(value.rec);post({type:'docs',keys:[['files',value.rec.id]]});}
    post({type:'upload_result',outcome:value.outcome});
  }
  async function confirm(k,e) {
    let awaitingResult=false;
    try {
      // Mark dispatch and validate the generation in one durable transaction.
      const sent = await db.tx(async t=>{await assertGen(t);const entry=await t.get('outbox',e.op_id);if(!entry)return false;await t.put('outbox',{...entry,sent_gen:myGen});return entry;});
      if(!sent || closed || !isLeader)return;
      e = sent;
      if(e.op==='upload') {
        const blob=await db.get('blobs',e.id); await transport.upload(e.id,blob.blob);
        const outcome=await db.tx(async t=>{await assertGen(t);await t.del('outbox',e.op_id);await t.put('blobs',{...blob,pinned:false});await t.put('meta',{key:`upload_done:${e.id}`,ref:e.id});const out=await storeUploadResult(t,e,{status:'accepted'});await bumpLocalSeq(t);return out;});
        publishUploadResult(outcome);
        setUploadProgress(e.id,blob.size,blob.size,true); uploadsDone.add(e.id);notify('uploads');post({type:'upload_done',ref:e.id});
      } else {
        if(!subscriptions.has(e.collection))await subscribeCollection(e.collection);
        if (e.collection === 'business_commands' && !e.pending_result) {
          commandTimings.mark(e.id, 'sent');
        }
        const result=e.pending_result??await transport.write(cloneValue(e));
        if(result)awaitingResult=!await applyResult(e,result);
        else awaitingResult=true;
      }
    } catch(err) {
      if(err instanceof StaleLeader)stepDown();
      else if(e.op === 'upload' && err?.core_rejection) {
        const out=await db.tx(async t=>{await assertGen(t);await t.del('outbox',e.op_id);const value=await storeUploadResult(t,e,{status:'rejected',code:err.code,reason:err.reason ?? err.message});await bumpLocalSeq(t);return value;});
        publishUploadResult(out);
        zoneReport(e.app_id,{level:'error',code:err.code,text:err.reason ?? err.message},'kernel');
      }
      else if(err?.code==='unavailable'){link={...link,state:'offline'};netChanged();}
      else {
        // An invalid receipt/scope needs a new admitted page or reconnect,
        // not an immediate resend loop of the same durable input.
        awaitingResult=true;
        zoneReport(e.app_id,{level:'error',code:err?.code??'transport.write',text:String(err.message)},'kernel');
      }
    } finally {
      if(!awaitingResult){if(inflight.get(k)===e.op_id)inflight.delete(k);inflightOps.delete(e.op_id);}
      await refreshOutboxStat(); if(!awaitingResult && link.state==='online')schedulePump();
    }
  }
  function canonicalReady(base,result) {
    return !!base && !!result.seq && (result.gen===undefined || base.gen===result.gen) && !seqGreater(result.seq,base.seq??'0');
  }
  async function settleWrite(t,e,base,res) {
    const local=e.local_version ?? {data:applyOp((await t.get('docs',[e.collection,e.id]))?.base??emptyBase(),e).data,deleted:e.op==='delete'};
    const changed=e.collection!=='business_commands' && (!jsonEq(local.data,base.data)||!!local.deleted!==!!base.deleted);
    if(res.status==='rejected' || changed){
      res.local_archived_ref=`archive_local:${e.op_id}`;
      if(res.status==='rejected')res.archived_ref=res.local_archived_ref;
      await t.put('archive_local',{...e,ref:res.local_archived_ref,local_version:local,canonical_version:cloneValue(base),reason:res.code??res.reason??'canonical_changed',rejected_at:Date.now()});
    }
    await t.del('outbox',e.op_id);
    const rec=recompute(e.collection,e.id,base,await t.outboxFor(e.collection,e.id));
    if (e.file_upload && (res.status === 'rejected' || (await t.get('meta',`upload_result:${e.file_upload_op}`))?.status === 'rejected') && !rec.pending) { rec.data={...e.body,state:'rejected'};rec.deleted=false;rec.local_file_rejection=true; }
    await putDoc(t,rec);
    const info={op_id:e.op_id,app_id:e.app_id,collection:e.collection,id:e.id};
    delete res.type;delete res.client_seq;delete res.requires_page;
    await t.put('meta',{key:`result:${e.op_id}`,info,result:res});
    return {rec,res,info};
  }
  function publishWrite(out) {
    mirrorPut(out.rec);setCheckpoint(out.rec.base.seq);
    const key=docKey(out.info.collection,out.info.id);
    if(inflight.get(key)===out.info.op_id)inflight.delete(key);
    inflightOps.delete(out.info.op_id);
    reservedOps.delete(out.info.op_id);
    handleWriteResult(out.info,out.res);
    post({type:'docs',keys:[[out.info.collection,out.info.id]]});post({type:'write_result',info:out.info,result:out.res});
    schedulePump();
  }
  async function applyResult(e,result) {
    const out=await db.tx(async t=>{
      await assertGen(t);
      const queued=await t.get('outbox',e.op_id);if(!queued)return null;
      const stored=await t.get('docs',[e.collection,e.id]); let base=stored?.base??emptyBase();
      const res={...result};delete res.doc;delete res.deleted;
      if(!['accepted','merged','rejected','migrated'].includes(res.status))throw new CtoxError('invalid','Invalid write result');
      const canonical=res.requires_page || res.status==='merged' || res.status==='migrated';
      if(canonical && res.status!=='rejected') {
        if(BigInt(decimalSeq(res.seq))===0n)throw new CtoxError('invalid','Canonical receipt needs a positive sequence');
        if(res.gen!==undefined && res.gen!==e.gen && res.status!=='migrated')throw new CtoxError('conflict','Write receipt generation changed');
        if(!canonicalReady(base,res)) {
          await t.put('outbox',{...e,pending_result:res});
          return {replay:true,res};
        }
      }
      if(res.status!=='rejected' && !canonical && !seqGreater(base.seq??'0',result.seq??'0')) {
        const state=result.doc!==undefined ? {data:result.doc,deleted:!!result.deleted} : applyOp(base,e);
        base={...state,rev:res.rev,seq:decimalSeq(res.seq),gen:e.gen};
      }
      return settleWrite(t,e,base,res);
    });
    if(!out)return true;
    if(e.collection === 'business_commands') commandTimings.mark(e.id, 'acked');
    if(out.replay) {
      // Explicit replay uses the SAME signed page path, without regressing any
      // durable checkpoint. A no-write command replay still needs its older row.
      const known=expectedScopes.get(e.collection);
      const scope=(!known || (out.res.gen!==undefined && known.gen!==out.res.gen)?await transport.scope?.(e.collection):known)??{collection:e.collection,epoch:opts.epoch??'1',scope_hash:opts.scopeHash??'local',gen:e.gen};
      if(out.res.gen!==undefined && scope.gen!==out.res.gen)throw new CtoxError('migration_required','Canonical receipt requires a current native scope',{path:e.collection});
      expectedScopes.set(e.collection,scope);
      await transport.subscribe({...scope,checkpoint_seq:String(BigInt(out.res.seq)-1n),priority:'P1'});
      return false;
    }
    publishWrite(out);
    const dw=doubleWrite.get(e.collection);
    if(fakeCore && dw && out.res.status!=='rejected')await corePush(dw.ns_new,[out.rec.base.deleted ? {id:e.id,_deleted:true} : {...dw.migrate(cloneValue(out.rec.base.data)),id:e.id}]);
    return true;
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
    const manifest = versionDoc(app.id, app.ver)?.manifest;
    const foreign = manifest?.foreign_collections?.[collection];
    const owner = foreign?.owner_app ?? foreign?.app ?? foreign?.owner;
    const ownerVer = owner && (visible(mirrorGet('business_apps',owner))?.active_ver ?? activations.get(owner)?.activated_ver);
    const g = (owner ? versionDoc(owner,ownerVer)?.manifest : manifest)?.generations?.[collection] ?? 1;
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
  async function receiveRevoke(message){
    for (const c of message.collections ?? []) if (collScopes.has(c)) { collScopes.get(c).error = { code: 'revoked', message: `Recht für ${c} entzogen` }; }
    if (refreshCollections()) netChanged();
    const collections=new Set(message.collections??[]),rows=await db.tx(async t=>{await assertGen(t);const changed=[];for(const row of await t.all('docs'))if(collections.has(row.collection)){const revoked={...row,data:null,base:emptyBase(),deleted:true,revoked:true};await putDoc(t,revoked);changed.push(revoked);}for(const hash of message.artifacts??[])await t.del('artifacts',hash);await t.put('meta',{key:'rights_ver',value:message.rights_ver});return changed;});
    for(const row of rows)mirrorPut(row);post({type:'docs',keys:rows.map(r=>[r.collection,r.id])});
  }
  function collectionVersion(app,collection) {
    const owner=SYSTEM_COLLECTIONS.has(collection)?PLATFORM_APP_ID:app.id;
    const ver=owner===PLATFORM_APP_ID?visible(mirrorGet('business_apps',owner))?.active_ver:owner===app.id?app.ver:activations.get(owner)?.activated_ver??visible(mirrorGet('business_apps',owner))?.active_ver;
    const manifest=versionDoc(owner,ver)?.manifest;
    return {owner,ver,definition:manifest?.collections?.[collection],gen:manifest?.generations?.[collection]};
  }
  function schemaFor(app, collection) { const {owner,ver,definition}=collectionVersion(app,collection);return (fakeCore && owner===PLATFORM_APP_ID && versionDoc(owner,ver)?.builtin ? PLATFORM_SCHEMAS[collection]??null : definition?.schema)??(fakeCore && owner===PLATFORM_APP_ID?PLATFORM_SCHEMAS[collection]??null:null); }
  function writeVersion(app,collection) {
    const {owner,ver,definition,gen}=collectionVersion(app,collection);
    // Wire versions describe the collection owner, never the calling app.
    // Production needs the authoritative platform catalog before a local write.
    if(!fakeCore && owner===PLATFORM_APP_ID && (!ver || !isPlainObject(definition?.schema)))throw new CtoxError('unavailable','Active platform catalog missing',{collection});
    return {schema_ver:definition?.schema?.version??(!fakeCore && owner===PLATFORM_APP_ID?0:1),app_ver:ver??(owner===PLATFORM_APP_ID?PLATFORM_APP.ver:app.ver),...(gen===undefined?{}:{gen})};
  }
  // §16.19 Nr. 3: shell_preferences – eine Zeile je Nutzer (id = actor.id), lesen und schreiben nur die eigene.
  function ownRowOk(collection, id) {
    if (!OWN_ROW_COLLECTIONS.has(collection)) return true;
    if (collection === 'app_drafts') return typeof id === 'string' && id.startsWith(`${userId}/`); // id = '<user>/<app>'
    return id === userId; // Identität des Kernels (actor.id kann nicht abweichen, §16.7)
  }

  // K-1c Punkt 9: kein busy. Veröffentlichungen einer App laufen nacheinander; die nächste wartet, bis die laufende
  // samt Aktivierung fertig ist, und endet dann selbst mit ihrem Ergebnis (pending → done/rejected).
  const publishChains = new Map();
  // fn liefert {value, hold}: value geht sofort an den Aufrufer, die Sperre hält bis hold (Aktivierung) erledigt ist.
  function withPublishLock(appId, fn) {
    const prev = publishChains.get(appId) ?? Promise.resolve();
    const run = prev.then(() => fn());
    const tail = run.then((r) => r?.hold).catch(() => {});
    publishChains.set(appId, tail);
    tail.then(() => { if (publishChains.get(appId) === tail) publishChains.delete(appId); });
    return run.then((r) => r.value);
  }
  async function settleActivation(appId, ver) {
    if (isLib(appId)) return;
    await waitUntil(() => isTerminalFor(appId, ver) || visible(mirrorGet('business_apps', appId))?.active_ver !== ver || closed || !isLeader);
  }
  function publishInternal(appId, ver, o = {}) {
    return withPublishLock(appId, async () => { const r = await publishCore(appId, ver, o); return { value: r, hold: settleActivation(appId, ver) }; });
  }
  async function publishCore(appId, ver, o = {}) {
    publishing.add(appId);
    try {
      const appDoc = visible(mirrorGet('business_apps', appId));
      if (o.baseAppVer !== undefined && (appDoc?.active_ver ?? null) !== o.baseAppVer) throw new CtoxError('conflict', 'stale_base');
      if(o.module!==undefined)throw new CtoxError('invalid','App modules belong to the Shell/testkit host');
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
        retires:{...(prevManifest?.retires??{})},
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
    if(appId==='ctox-platform' && versionDoc(appId,visible(mirrorGet('business_apps',appId))?.active_ver)?.builtin)return; // resident Core service, no app mount or eager data subscriptions

    if (!isLeader || closed || isLib(appId)) return; // §16.37: lib:<name> nie eager, nur dateiweise bei SW-Miss
    const prev = activationChains.get(appId) ?? Promise.resolve();
    const next = prev.then(() => runActivation(appId)).catch(async(err) => {
      if (err instanceof StaleLeader) stepDown();
      else if (!closed) {await setActivation(appId,{state:'failed',error:err?.message??'Collection grant missing'});zoneReport(appId,{level:'error',code:err?.code??'unavailable',text:err?.message??'Collection grant missing'},'kernel');}
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
    try{if(typeof transport.grantApp!=='function')throw new CtoxError('forbidden','Collection grant missing');const grant=await transport.grantApp(appId);if(typeof grant!=='string'||!grant)throw new CtoxError('forbidden','Collection grant missing');}catch(error){await setActivation(appId,{target_ver:ver,state:'failed',error:'Collection grant missing'});zoneReport(appId,{level:'error',code:'forbidden',text:'Collection grant missing'},'kernel');return;}
    // fetched: alle Dateien per Hash geprüft (aus „Spur 4“ = transport CAS)
    for(const [path,hash]of Object.entries(manifest.files??{})){
      const cached=await db.get('artifacts',hash);
      if(cached && await sha256Hex(cached.content)===hash)continue;
      await waitUntil(() => link.state === 'online' || closed); if (closed) return;
      // §16.15a Nr. 3 (wie Stub): sha256-Fehlschlag → einmal neu über Spur 4 holen, erst wiederholter Fehlschlag → failed.
      // Jeder Zweig endet in setActivation; kein Warten ohne Ende (K-1c Punkt 0). Transport offline → Fehler nach oben.
      let content = null;
      for (let attempt = 0; attempt < 2 && content === null; attempt++) {
        let got = await transport.artifact(hash).then(artifactContent).catch((error) => { if (error?.code === 'unavailable') throw error; return error?.reason === 'hash_mismatch' ? HASH_MISMATCH : null; });
        if (got === HASH_MISMATCH) { simRecord.refetches++; continue; }
        if (got == null) { await setActivation(appId, { target_ver: ver, state: 'failed', error: `artifact_missing:${path}` }); return; }
        if (corruptNextFetch > 0) { corruptNextFetch--; got = typeof got === 'string' ? `${got}\n/* beschädigt */` : Uint8Array.from([...got, 0]); }
        if ((await sha256Hex(got)) === hash) content = got;
        else simRecord.refetches++;
      }
      if (content === null) { await setActivation(appId, { target_ver: ver, state: 'failed', error: `hash_mismatch:${path}` }); return; }
      await db.tx(async (t) => { await assertGen(t); await t.put('artifacts', { hash, content, app_id: appId, app_ver: ver, path, stored_at: Date.now() }); });
    }
    await setActivation(appId, { target_ver: ver, state: 'fetched' });
    // E43.2: a transport that grants capabilities in hello_ok reports null until then; never judge before the grant.
    if ((manifest.capabilities_required ?? []).some((c) => !capabilities.has(c)) && transport.capabilities === null) { await waitUntil(() => transport.capabilities !== null || closed); if (closed) return; }
    const missing = (manifest.capabilities_required ?? []).filter((c) => !capabilities.has(c) && !(transport.capabilities ?? []).includes(c)); // Katalog-Primitive des Core (§16.29)
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
    await writeFilesMeta(appId, ver, manifest.files); // K-1d: SW löst Pfad → sha256 aus der Kernel-IDB auf
    const previous=activations.get(appId);
    await setActivation(appId,{target_ver:ver,activated_ver:ver,previous_ver:previous?.activated_ver??null,good_vers:[...(previous?.good_vers??[]).filter(v=>v!==ver),ver].slice(-10),state:'activated',at_seq:checkpoint});
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
    simRecord.activationReports.push({ app_id: appId, app_ver: value.target_ver, state: value.state, at_seq: checkpoint, error: value.error ?? undefined });
    applyActivationLocal(value, true);
    post({ type: 'activation', value });
    sendActivationReport(value);
    return value;
  }
  function sendActivationReport(value){
    const report={app_id:value.app_id,app_ver:value.target_ver,state:value.state,at_seq:checkpoint,...(value.state==='failed'&&value.error?{error:value.error}:{})};
    activationReports=activationReports.then(()=>{if(!closed&&isLeader)return transport.appActivation(report);}).catch(error=>{if(!closed)zoneReport('kernel',{level:'warn',code:'activation.report',text:error.message},'kernel');});
    track(activationReports);
  }
  function applyActivationLocal(value, fromSelf) {
    const prev = activations.get(value.app_id);
    if (prev && prev.updated_at > value.updated_at) return;
    if (prev && jsonEq(prev, value)) return;
    activations.set(value.app_id, value);
    notify('activation');
    const ev = Object.freeze({ app_id: value.app_id, ver: value.target_ver, state: value.state, activated_ver: value.activated_ver, error: value.error ?? null });
    for (const cb of [...activationListeners]) { try { cb(ev); } catch (e) { console.error('[ctox-kernel] activation listener', e); } }
    // K-1c Punkt 8: eine aktivierte Version erledigt Kernel-Meldungen zu anderen (älteren) Versionen in JEDEM Tab
    if (value.state === 'activated') clearStaleVersionEntries(value.app_id, value.activated_ver);
    if (value.state === 'failed' && value.error?.startsWith('needs_update')) {
      zoneReport(value.app_id, { level: 'error', code: 'app.needs_update', text: `App ${value.app_id}@${value.target_ver} benötigt ein Update dieses Teilnehmers (${value.error.slice(13)})`, ref: value.target_ver }, 'kernel');
    } else if (value.state === 'failed' && value.error) {
      zoneReport(value.app_id, { level: 'error', code: 'app.activation_failed', text: `Aktivierung ${value.app_id}@${value.target_ver} fehlgeschlagen: ${value.error}`, ref: value.target_ver }, 'kernel');
    }
    if(isLeader && value.state==='activated' && value.previous_ver && value.previous_ver!==value.activated_ver) {
      later(()=>track(retireGenerations(value.app_id,value.activated_ver,value.previous_ver)),generationGraceMs);
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
          const empty={collection:row.collection,id:row.id,data:null,base:emptyBase(),rev:null,seq:'0',deleted:true,pending:false,retired:true};await putDoc(t,empty);await t.del('docs',[ns,row.id]);await t.put('meta',{key:`retired:${ns}:${row.id}`,record:empty});out.push(empty);
        }
      }
      return out;
    });
    for(const row of retired){mirrorPut(row);doubleWrite.delete(row.collection);}
    post({type:'docs',keys:retired.map(r=>[r.collection,r.id])});
  }

  // ---- Mount / Remount (§10.2a Nr. 1, §16.10) ----
  // K-1d (Entscheidung: Kernel-IDB ist maßgeblich, der SW passt sich an): meta `files:<appId>@<ver>` = {pfad: sha256};
  // der SW liest daraus die sha256 und dann artifacts[sha256].content (string | Uint8Array). Kein Store `manifests`.
  async function writeFilesMeta(appId, ver, files) {
    const key = `files:${appId}@${ver}`, value = { ...(files ?? {}) };
    const cur = await db.get('meta', key);
    if (cur && jsonEq(cur.value, value)) return;
    await db.tx((t) => t.put('meta', { key, value }));
  }
  // binding = die beim Mount erzeugte Bindung (Shell-Host/testkit); ohne: eine neue Bindung je Ladevorgang (Mount)
  async function loadModule(appId, ver, binding = null) {
    const shellVer=appId==='shell'?ver:binding?.shellVer??activations.get('shell')?.activated_ver??'0';
    const key = binding ? binding.root : `${appId}@${ver}~shell@${shellVer}`;
    if (moduleCache.has(key)) return moduleCache.get(key);
    const manifest = versionDoc(appId, ver)?.manifest;
    if (!manifest) throw new CtoxError('not_found', `Manifest ${key} fehlt`);
    const hash = manifest.files?.[manifest.entry];
    const art = hash ? await db.get('artifacts', hash) : null;
    if (!art) throw new CtoxError('not_found', `Einstiegsdatei ${manifest.entry} von ${key} nicht lokal`);
    let url, revoke = null;
    if (globalThis.navigator?.serviceWorker?.controller) {
      url = (binding ?? bindArtifacts(appId, ver)).url(manifest.entry); // §16.39: Wurzel mit Shell- und Lib-Bindung
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
  // ---- Datei-Blobs ----
  function setUploadProgress(ref, sent, total, broadcast) {
    uploadProgress.set(ref, { sent, total });
    notify(`upload:${ref}`);
    if (broadcast) post({ type: 'upload', ref, sent, total });
  }

  async function fetchRange(ref, range) {
    guardAlive();
    if (!isLeader || link.state !== 'online') throw new CtoxError('unavailable', 'File not cached and leader offline');
    const bytes = new Uint8Array(await (await transport.blob(ref, range)).arrayBuffer());
    if (bytes.length !== range.end - range.start) throw new CtoxError('invalid', 'Incomplete file range');
    return bytes;
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
      registerHeader(el){if(el!==null && (typeof HTMLElement==='function'?!(el instanceof HTMLElement):!el||typeof el!=='object'||typeof el.tagName!=='string'))throw new CtoxError('invalid','Header must be an HTMLElement or null');win.header=el;notify(winTag);},
      canNavigate(target, params = {}) {
        const known = target === 'shell' || !!visible(mirrorGet('business_apps', target));
        if(!known)return false;
        if(params.command_type!==undefined)return (readManifest(appId,app.ver)?.commands_out??[]).some(x=>x.app===target&&x.type===params.command_type);
        return true;
      },
      navigate(target, params = {}) {
        const known = shell.canNavigate(target,params);
        if (!known) {
          zoneReport(appId, { level: 'error', code: 'shell.navigate.unknown_target', text: `Navigationsziel unbekannt: ${target}`, ref: String(target) }, 'kernel');
          return false;
        }
        if (params.ref !== undefined) return (async () => {
          const sel = typeof params.ref === 'string' ? {ref:params.ref} : params.ref;
          const file = await readableFile(sel, false);
          const grant = {from:appId,to:target,ref:file.ref,user_id:userId,expires_at:Date.now()+3600000,at_ms:Date.now()};
          // Audit commits before delivery. A failed authorization never emits a route.
          await db.tx(t=>t.put('meta',{key:`file-grant:${uuidv7()}`,value:{...grant,event:'granted'}}));
          fileReadGrants.set(JSON.stringify([target,file.ref]),grant);
          return emitRoute();
        })();
        return emitRoute();
        function emitRoute() {
        const ev = Object.freeze({ from: appId, to: target, params: cloneValue(params) });
        simRecord.navigations.push(ev);
        for (const cb of [...(routeListeners.get(target) ?? [])]) { try { cb(ev); } catch (e) { console.error(e); } }
        return true;
        }
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
    // ---- K-1g: files nach CAP-3b (#844) – Ergebnisse immer Objekte, nie Base64, nur Dateien der eigenen App ----
    const FILE_TEXT_MAX = 512 * 1024;
    const fileDocs = () => [...(mirror.get('files')?.values() ?? [])].map((r) => visible(r)).filter(Boolean);
    function ownFile(sel) {
      if (!isPlainObject(sel) || (typeof sel.id !== 'string' && typeof sel.ref !== 'string')) throw new CtoxError('invalid', 'files: {id} oder {ref} nötig', { path: 'ref' });
      const docs = fileDocs().filter((d) => (sel.id ? d.id === sel.id : d.ref === sel.ref));
      const mine = docs.find((d) => d.owner_app === appId);
      if (!mine && docs.length) throw new CtoxError('forbidden', 'Datei gehört einer anderen App', { path: sel.id ? 'id' : 'ref' });
      return mine ?? null;
    }
    // Grants come from Core-confirmed manifests, never optimistic metadata.
    const readManifest = (id, ver) => {
      const row = mirrorGet('business_app_versions',`${id}@${ver}`);
      return !row?.revoked && !row?.base?.deleted ? row?.base?.data?.manifest : undefined;
    };
    const currentManifest = owner => {
      const row = mirrorGet('business_apps',owner);
      const ver = !row?.revoked && !row?.base?.deleted ? row?.base?.data?.active_ver : undefined;
      return readManifest(owner,ver);
    };
    const ownReferenceRows = new Set(['shell_preferences','app_drafts']);
    function readableNamespace(ns) {
      const caller = readManifest(appId,app.ver);
      const c = ns.replace(/@g[1-9][0-9]*$/,'');
      if (c === PREFERENCE_COLLECTION) return false; // preferences never grant another app a file reference
      if (caller?.collections?.[c] && (!SYSTEM_COLLECTIONS.has(c) || appId==='ctox-platform')) return ns === resolveNs(app,c);
      const f = caller?.foreign_collections?.[c];
      const owner = f?.owner_app ?? f?.app ?? f?.owner;
      const om = owner && currentManifest(owner);
      return !!(f?.ops?.includes('read') && om?.collections?.[c] && om?.exports?.[c]?.includes('read') && ns === nsName(c,om.generations?.[c] ?? 1));
    }
    // §16.13: known owners and declared foreign reads require both grants.
    // Published callers fail closed on undeclared names, even before the owner
    // manifest arrives. Only manifest-less contract/bootstrap names are local.
    function assertReadableCollection(collection, allowPending = false) {
      const c = collection.replace(/@g[1-9][0-9]*$/, '');
      if (c === PREFERENCE_COLLECTION) throw new CtoxError('forbidden','Use api.prefs',{path:'collection'});
      const callerManifest = readManifest(appId,app.ver);
      const foreign = callerManifest?.foreign_collections?.[c];
      const owner = foreign?.owner_app ?? foreign?.app ?? foreign?.owner;
      const ownerRow = owner && mirrorGet('business_apps',owner);
      const ownerVersion = ownerRow?.base?.data?.active_ver;
      const versionRow = owner && mirrorGet('business_app_versions',`${owner}@${ownerVersion}`);
      // A declared read can wait for installation; absence is never authority to read rows.
      if(allowPending && foreign?.ops?.includes('read') && owner && !ownerRow?.revoked && !ownerRow?.base?.deleted && !versionRow?.revoked && !versionRow?.base?.deleted && !currentManifest(owner))return;
      const owners = [...(mirror.get('business_app_versions')?.values() ?? [])]
        .filter(row => !row.revoked && !row.base?.deleted).map(row => row.base?.data).filter(Boolean).filter(v => v.app_id !== appId && v.manifest?.collections?.[c]);
      if ((foreign || owners.length || (SYSTEM_COLLECTIONS.has(c) && appId!=='ctox-platform') || (callerManifest && !callerManifest.collections?.[c])) && !readableNamespace(resolveNs(app, collection)))
        throw new CtoxError('forbidden', 'Foreign collection requires declaration and read export', {path:'collection'});
    }
    function readableLive(collection, tags, compute, empty) {
      const live = makeLive([...tags,'c:business_apps','c:business_app_versions','activation'], depend => {
        try { assertReadableCollection(collection); }
        catch (error) { if (error.code === 'forbidden') return empty; throw error; }
        return compute(depend);
      });
      return {
        get() { assertReadableCollection(collection,true); return live.get(); },
        subscribe(cb) { assertReadableCollection(collection,true); return live.subscribe(cb); },
      };
    }
    // E30: a CAS hash is not authority. Only canonical, still-readable rows
    // grant access; an optimistic reference cannot mint a read grant.
    async function readableFile(sel, consumeGrant = true) {
      guardAlive();
      if (!isPlainObject(sel) || (typeof sel.id !== 'string' && typeof sel.ref !== 'string')) throw new CtoxError('invalid', 'files: {id} oder {ref} nötig', {path:'ref'});
      const docs = fileDocs().filter(d => sel.id ? d.id === sel.id : d.ref === sel.ref);
      for (const d of docs) {
        if (d.owner_app !== appId) continue;
        const row = mirrorGet('files',d.id), base = row?.base;
        if (row?.revoked) continue;
        if (!base?.deleted && base?.data?.owner_app === appId && base.data.ref === d.ref) return d;
        // files.put proves local bytes in the blob transaction, durably across
        // reload. A plain optimistic files row cannot claim another app's CAS.
        if ((row?.pending || row?.local_file_rejection) && (await db.get('meta',`file-owner:${JSON.stringify([appId,d.id])}`))?.ref === d.ref) return d;
      }
      const canonicalRows = [];
      for (const [ns, rows] of mirror) {
        const declared = readableNamespace(ns);
        if (!declared && ns !== 'business_commands' && !ownReferenceRows.has(ns)) continue;
        for (const row of rows.values()) if (!row.deleted && !row.revoked && !row.base?.deleted && row.base?.data && ownRowOk(ns,row.id)) {
          const d = row.base.data;
          // Standard API handles already expose own command results and own
          // platform rows; they need no foreign-collection declaration.
          if (declared || ownReferenceRows.has(ns) || (ns === 'business_commands' && d.app_id === appId && d.actor_id === userId)) canonicalRows.push([ns,d]);
        }
      }
      const desktop = canonicalRows.find(([ns,d]) => ns.replace(/@g[1-9][0-9]*$/,'') === 'desktop_files' && d.id === sel.id && !d.is_deleted)?.[1];
      if (desktop) {
        const ref = [desktop.ref,desktop.content_ref,docs[0]?.ref].find(v => typeof v === 'string' && /^[a-f0-9]{64}$/.test(v));
        if (typeof ref !== 'string' || !/^[a-f0-9]{64}$/.test(ref)) throw new CtoxError('unavailable','Desktop file has no CAS reference',{path:'id'});
        const meta = fileDocs().find(d => d.ref === ref);
        return {...meta,id:desktop.id,ref,name:desktop.name ?? meta?.name,mime:desktop.mime ?? desktop.mime_type ?? meta?.mime,size:undefined, verify_size:true};
      }
      const ref = docs[0]?.ref ?? sel.ref;
      const containsRef = value => value !== null && typeof value === 'object' && (value.ref === ref || value.blob_ref === ref || Object.values(value).some(containsRef));
      if (typeof ref === 'string' && /^[a-f0-9]{64}$/.test(ref) && canonicalRows.some(([,d]) => containsRef(d))) return docs[0] ?? {ref,id:ref,name:null};
      const grantKey = JSON.stringify([appId,ref]);
      const grant = fileReadGrants.get(grantKey);
      if (consumeGrant && grant?.user_id===userId && grant.expires_at>Date.now()) {
        fileReadGrants.delete(grantKey); // consume before any await: concurrent reads cannot reuse it
        await db.tx(t=>t.put('meta',{key:`file-grant:${uuidv7()}`,value:{...grant,event:'consumed',at_ms:Date.now()}}));
        return docs[0] ?? {id:ref,ref,name:null};
      }
      throw new CtoxError('forbidden','File needs ownership or a readable reference',{path:sel.id ? 'id' : 'ref'});
    }
    const fileInfo = (d) => ({ id: d.id, ref: d.ref, name: d.name ?? null, mime: d.mime ?? 'application/octet-stream', size: d.size ?? 0 });
    async function fileBytes(content) {
      if (isPlainObject(content) && typeof content.text === 'string') return { bytes: new TextEncoder().encode(content.text), text: true };
      if (content instanceof ArrayBuffer) return { bytes: new Uint8Array(content.slice(0)), text: false };
      if (ArrayBuffer.isView(content)) return { bytes: new Uint8Array(content.buffer.slice(content.byteOffset, content.byteOffset + content.byteLength)), text: false };
      throw new CtoxError('invalid', 'files.put: Inhalt muss Uint8Array/ArrayBuffer oder {text} sein', { path: 'content' });
    }
    const files = Object.freeze({
      /** CAP-3b: put(bytes | {text}, {name, mime}) → {id, ref} nach lokalem Commit (Upload über Spur 4). Blob = Altform (Handle). */
      put(content, meta = {}) {
        if (isBlob(content)) return putBlob(content, meta);
        return (async () => {
          guardAlive();
          const { bytes, text } = await fileBytes(content);
          const mime = String(meta.mime ?? (text ? 'text/plain; charset=utf-8' : 'application/octet-stream'));
          const id = typeof meta.id === 'string' && meta.id ? meta.id : uuidv7();
          const h = putBlob(new Blob([bytes], { type: mime }), { ...meta, id, mime, name: meta.name ?? 'datei' });
          const ref = await h.ref;
          await h.local;
          return Object.defineProperty({id,ref},'done',{value:h.done});
        })();
      },
      /** CAP-3b: get({id|ref}) → {found, id, ref, name, size, mime, text? | bytes?}; Text nur für UTF-8 ≤ 512 KiB. String = Altform (Blob). */
      async get(sel) {
        if (typeof sel === 'string') return getBlob((await readableFile({ref:sel})).ref);
        guardAlive();
        let d = await readableFile(sel);
        if (sel.range !== undefined && (d.verify_size || !Number.isSafeInteger(d.size))) {
          const blob = await getBlob(d.ref);
          if (await sha256Hex(blob) !== d.ref) throw new CtoxError('invalid','File content hash mismatch');
          d = {...d,size:blob.size};
        }
        if (!d) return { found: false, ref: sel.ref ?? null, ...(sel.id ? { id: sel.id } : {}) };
        if (sel.range !== undefined) {
          const {start, end} = sel.range ?? {};
          if (!isPlainObject(sel.range) || !Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || end < start || end > d.size) throw new CtoxError('invalid', 'Byte range outside file', {path:'range'});
          const local = await db.get('blobs', d.ref);
          let bytes;
          if (local) {
            // The immutable cached Blob was verified on full receipt/upload.
            await db.tx(t => t.put('blobs', {...local, last_used:Date.now()}));
            bytes = new Uint8Array(await local.blob.slice(start, end).arrayBuffer());
          } else bytes = isLeader ? await fetchRange(d.ref, sel.range) : await callLeader('fetchRange', [d.ref, sel.range]);
          return {found:true, ...fileInfo(d), bytes};
        }
        const bytes = new Uint8Array(await (await getBlob(d.ref)).arrayBuffer());
        let text;
        if (bytes.byteLength <= FILE_TEXT_MAX) { try { text = new TextDecoder('utf-8', { fatal: true }).decode(bytes); } catch { text = undefined; } }
        return { found: true, ...fileInfo(d), size: bytes.byteLength, ...(text !== undefined ? { text } : { bytes }) };
      },
      /** CAP-3b: list() → {files:[{id, ref, name, mime, size}]} – nur die eigene App. */
      async list() {
        guardAlive();
        return { files: fileDocs().filter((d) => d.owner_app === appId).map(fileInfo).sort((a, b) => (a.id < b.id ? -1 : 1)) };
      },
      /** CAP-3b: delete({id|ref}) → {deleted, id?} – nur die eigene App. */
      async delete(sel) {
        guardAlive();
        const d = ownFile(sel);
        if (!d) return { deleted: false };
        const r = await localWrite(app, 'delete', 'files', d.id, null).local.then(() => true, () => false);
        return { deleted: r, id: d.id };
      },
    });
    async function getBlob(ref) {
      {
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
      }
    }
    function putBlob(blob, meta = {}) {
      {
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
            await t.put('outbox', { op_id, collection: '__blobs', id: ref, op: 'upload', body: null, file_id:id, app_id: appId, app_ver: app.ver, created_at: Date.now(), origin_tab: tabId });
            await t.put('meta',{key:`file-owner:${JSON.stringify([appId,id])}`,ref});
          });
          if(cancelledEarly){await db.tx(async t=>{await t.del('outbox',op_id);const b=await t.get('blobs',ref);if(b)await t.put('blobs',{...b,pinned:false});});return;}
          post({ type: 'outbox' });
          if (isLeader) schedulePump();
          metaHandle = localWrite(app, 'put', 'files', id, {
            id, name: String(meta.name ?? 'datei'), mime: String(meta.mime ?? (blob.type || 'application/octet-stream')),
            size: blob.size, ref, owner_app: appId,
          }, false, {file_upload:true,file_upload_op:uploadOp});
          await metaHandle.local;
        })();
        local.then(async () => {
          if (cancelledEarly || !metaHandle) { resolveResult({ status: 'rejected', reason: 'cancelled' }); return; }
          const r = await metaHandle.result;
          if (r.status !== 'rejected') await waitUntil(() => uploadResults.has(uploadOp) || closed);
          const upload = uploadResults.get(uploadOp);
          const final = r.status === 'rejected' ? r : upload?.status === 'accepted' ? r : {status:'rejected',code:upload?.code ?? 'unavailable',reason:upload?.reason ?? 'Upload not confirmed before close'};
          resolveResult({ ...final, ref: refValue });
        }, (err) => resolveResult({ status: 'rejected', reason: err instanceof CtoxError ? err.code : 'local_error' }));
        local.catch(() => {});
        const done = result.then(r => {
          if (r.status === 'rejected') throw new CtoxError(r.code ?? r.reason ?? 'unavailable', r.reason ?? 'File rejected', {reason:r.reason});
          return r;
        });
        done.catch(() => {}); // consumers may attach after the local acknowledgement
        return {
          local, result, ref: refP, done,
          upload: Object.freeze({ progress }),
          cancel() {
            if (!metaHandle) { cancelledEarly = true; return true; }
            if(uploadOp && inflightOps.has(uploadOp))return false;
            const cancelled=metaHandle.cancel();
            if(cancelled && uploadOp)track(db.tx(async t=>{await t.del('outbox',uploadOp);const b=await t.get('blobs',refValue);if(b)await t.put('blobs',{...b,pinned:false});}));
            return cancelled;
          },
        };
      }
    }
    const actorApi = Object.freeze({
      get id() { return actor.id; },
      get display_name() { return actor.display_name; },
      get role() { return actor.role; },
      permissions: makeLive(['actor'], () => [...actor.permissions]),
    });
    return Object.freeze({
      version: APP_API_VERSION,
      app,
      prefs: createPreferencesApi({appId,userId,read:id=>visible(mirrorGet(PREFERENCE_COLLECTION,id)),write:(op,id,body)=>localWrite(app,op,PREFERENCE_COLLECTION,id,body,false,{preference:true})}),
      query(collection, q = {}) {
        assertReadableCollection(collection,true);
        const cq = compileQuery(q);
        const ns = resolveNs(app, collection);
        lastUsed.set(ns,Date.now());
        if(evicted.has(ns)){if(isLeader)track(subscribeCollection(ns,'P1'));else post({type:'subscribe',collection:ns});}
        return readableLive(collection, [`c:${ns}`, 'actor'], depend => {
          const ns=resolveNs(app,collection);depend(`c:${ns}`);lastUsed.set(ns,Date.now());
          const m = mirror.get(ns);
          if (!m) return [];
          const docs = [];
          for (const r of m.values()) if (r.data != null && !r.deleted && ownRowOk(ns, r.id)) docs.push(readDocument(r,withDefaults(app,collection,r.data)));
          return runCompiledQuery(docs, cq);
        }, []);
      },
      get(collection, id) {
        assertReadableCollection(collection,true);
        const ns = resolveNs(app, collection);
        lastUsed.set(ns,Date.now());
        if(evicted.has(ns)){if(isLeader)track(subscribeCollection(ns,'P1'));else post({type:'subscribe',collection:ns});}
        return readableLive(collection, [`c:${ns}`, 'actor'], depend => {const ns=resolveNs(app,collection);depend(`c:${ns}`);lastUsed.set(ns,Date.now());return ownRowOk(ns,id) ? readDocument(mirrorGet(ns,id),withDefaults(app,collection,visible(mirrorGet(ns,id)))) : null;}, null);
      },
      put(collection, doc) { return localWrite(app, 'put', collection, doc?.id, doc); },
      patch(collection, id, patch, options = {}) { return localWrite(app, 'patch', collection, id, patch, false, {expected_revision:options.expected_revision}); },
      remove(collection, id) { return localWrite(app, 'delete', collection, id, null); },
      command(type, payload = {}, commandOptions={}) {
        if (typeof type !== 'string' || !type) throw new CtoxError('invalid', 'Befehlstyp fehlt', { path: 'type' });
        const route=commandTarget({type,app_id:appId,...(commandOptions.target_app===undefined?{}:{target_app:commandOptions.target_app})},readManifest(appId,app.ver));
        const key = commandOptions.idempotency_key;
        if (key !== undefined && (typeof key !== 'string' || key.length > 128)) throw new CtoxError('invalid', 'idempotency_key must be a string of at most 128 characters', { path: 'idempotency_key' });
        // UUIDv8: first 128 SHA-256 bits, RFC variant/version bits normalized.
        const hash = key === undefined ? null : sha256Js(new TextEncoder().encode(`${appId}|${actor.id}|${key}`));
        const id = hash === null ? uuidv7() : `${hash.slice(0,8)}-${hash.slice(8,12)}-8${hash.slice(13,16)}-${((parseInt(hash[16],16)&3)|8).toString(16)}${hash.slice(17,20)}-${hash.slice(20,32)}`;
        cmdSubmitFailed.delete(id);
        const write = localWrite(app, 'put', 'business_commands', id, {
          id, ...(commandOptions.record_id===undefined?{}:{record_id:commandOptions.record_id}),
          // §16.39 Nr. 3: Rückruf-Befehlstypen der auslösenden App als Felder am Befehlsdokument (Core-PR #848 liest sie dort)
          ...(typeof commandOptions.on_complete === 'string' ? { on_complete: commandOptions.on_complete } : {}),
          ...(typeof commandOptions.on_failed === 'string' ? { on_failed: commandOptions.on_failed } : {}),
          type, payload, status: 'queued', app_id: appId, actor_id: actor.id, created_at: new Date().toISOString(),
          ...(commandOptions.target_app!==undefined ? {target_app:commandOptions.target_app} : !route.error && !route.platform && route.target!==appId ? {target_app:route.target} : {}),
        }, key !== undefined);
        write.local.catch(error=>{cmdSubmitFailed.set(id,{status:'submit_failed',error:{code:error?.code??'local_error',reason:error?.message??String(error)},local_inserted:false});notify('commands');});
        const status = makeLive(['c:business_commands', 'commands'], () => {
          if(cmdSubmitFailed.has(id))return cmdSubmitFailed.get(id);
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
      /** #830/CATALOG §6: model.run(request) → Responses-förmige Antwort; nur mit `model` in capabilities_required.
       *  Fake-Core: deterministische simulierte Antwort; sonst unavailable (der Modellweg gehört der App-Laufzeit, kein Ersatz). */
      model: Object.freeze({
        async run(req) {
          guardAlive();
          const declared = versionDoc(appId, app.ver)?.manifest?.capabilities_required ?? [];
          if (!declared.includes('model') && !declared.includes('model.run')) throw new CtoxError('forbidden', 'model nicht in capabilities_required', { path: 'model' });
          if (!fakeCore) throw new CtoxError('unavailable', 'model.run läuft über die App-Laufzeit (#830); kein Ersatzweg im Kernel');
          return fakeCore.runtime.modelRun(appId, req);
        },
      }),
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
  // ---- Quelltext-API (§10.2a Nr. 7, §16.22 A1–A9, §16.26, §16.39 Nr. 4) ----
  // Entwurf je (App, Nutzer) als Dokument der Plattform-Collection app_drafts (id '<user>/<app>', wie der Stub), über den
  // normalen Schreibweg (Outbox → Core). files: Pfad → sha256 (Inhalt im IDB-Store artifacts, Text oder Bytes),
  // null = im Entwurf gelöscht. manifest.json ist keine Datei; Manifest-Felder nur über draft.setManifest (manifest_fields).
  const draftId = (appId) => `${userId}/${appId}`;
  const activeVerOf = (appId) => visible(mirrorGet('business_apps', appId))?.active_ver ?? null;
  const asText = (c) => (typeof c === 'string' ? c : new TextDecoder().decode(c));
  function noManifestFile(path, field = 'path') {
    if (path === MANIFEST_FILE) throw new CtoxError('invalid', 'manifest.json ist im Entwurf keine Datei – Felder über draft.setManifest (§16.26)', { path: field });
  }
  function freshDraft(appId) {
    const base = activeVerOf(appId);
    const d = { app_id: appId, base_app_ver: base, files: {}, manifest: {} };
    // §16.26: eine Basisdatei manifest.json wird beim Anlegen des Entwurfs verworfen
    if (base && versionDoc(appId, base)?.manifest?.files?.[MANIFEST_FILE]) d.files[MANIFEST_FILE] = null;
    return d;
  }
  const docToDraft = (doc) => ({ app_id: doc.app_id, base_app_ver: doc.base_app_ver ?? null, files: cloneValue(doc.files ?? {}), manifest: cloneValue(doc.manifest_fields ?? {}), updated_at: doc.updated_at_ms ?? null });
  const draftDoc = (appId) => visible(mirrorGet('app_drafts', draftId(appId)));
  // Entwurfsinhalt in den lokalen CAS (IDB artifacts); Rückgabe sha256.
  async function storeDraftContent(appId, path, content) {
    const hash = await sha256Hex(content);
    await db.tx((t) => t.put('artifacts', { hash, content, app_id: appId, app_ver: 'draft', path, draft: true, size: contentBytes(content), stored_at: Date.now() }));
    fakeCore?.putArtifact(hash, content); // Fake-Core: Entwurfsinhalt im Core-CAS (ctox.app.publish liest ihn dort; echter Core: Upload)
    return hash;
  }
  const draftContent = async (hash) => (await db.get('artifacts', hash))?.content ?? readArtifactContent(hash);
  // K-1/K-1b-Entwürfe in IDB meta `draft:<app>` (Inhalte inline) einmalig nach app_drafts überführen.
  async function migrateLegacyDraft(appId) {
    const old = await db.get('meta', `draft:${appId}`);
    if (!old || draftDoc(appId)) { if (old) await db.tx((t) => t.del('meta', `draft:${appId}`)); return; }
    const files = {};
    for (const [p, c] of Object.entries(old.files ?? {})) files[p] = c === null ? null : await storeDraftContent(appId, p, c);
    const d = Object.prototype.hasOwnProperty.call(old, 'base_app_ver') ? { app_id: appId, base_app_ver: old.base_app_ver, files, manifest: old.manifest ?? {} } : { ...freshDraft(appId), files };
    await writeDraft(appId, d).local;
    await db.tx((t) => t.del('meta', `draft:${appId}`));
  }
  /** Eigener Entwurf (files = Hashes) oder null. */
  async function draftOf(appId) {
    await migrateLegacyDraft(appId);
    const doc = draftDoc(appId);
    return doc ? docToDraft(doc) : null;
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
  function writeDraft(appId, d) {
    const doc = { id: draftId(appId), app_id: appId, user_id: actor.id, base_app_ver: d.base_app_ver ?? null, files: d.files, manifest_fields: d.manifest ?? {}, updated_at_ms: Date.now() };
    return localWrite(PLATFORM_APP, 'put', 'app_drafts', doc.id, doc);
  }
  // Lese-Ändere-Schreib-Schritt auf dem eigenen Entwurf: Spiegel lesen, ändern und optimistisch schreiben geschieht
  // synchron (localWrite.prepare), damit parallele Aufrufe (auch anderer Tabs über den Spiegel) kein Update verlieren.
  async function draftMutate(appId, mutate) {
    requirePerm('apps.source.edit');
    await migrateLegacyDraft(appId);
    const doc = draftDoc(appId);
    const d = doc ? docToDraft(doc) : freshDraft(appId);
    mutate(d);
    const h = writeDraft(appId, d);
    await h.local;
    return h;
  }
  /** §16.44-Ergänzung (3): {local, result}; result = vom Core bestätigte Revision {status:'accepted', rev} bzw. rejected{code}. */
  function draftHandle(work) {
    const local = work.then(() => undefined);
    const result = work.then(async (h) => {
      const r = await h.result;
      return r.status === 'rejected' ? { status: 'rejected', code: r.code ?? r.reason, reason: r.reason } : { status: 'accepted', rev: r.rev };
    }, (e) => ({ status: 'rejected', code: e?.code ?? 'local_error', reason: String(e?.message ?? e) }));
    // Ein echtes Promise (auf den lokalen Commit) mit den Feldern local und result: `await`, `.catch` und
    // assert.rejects bleiben wie bisher (Shell-Quelltextmodus nutzt write(...).catch).
    local.catch(() => {}); // wer nur result liest, erzeugt keinen unbehandelten Fehler; await/.catch sehen ihn weiterhin
    local.local = local;
    local.result = result;
    return local;
  }
  // §16.44-Ergänzung (2): Revision nur für eine bestätigte, nicht gelöschte Zeile ohne ausstehende Änderungen (sonst null)
  function confirmedDraftRev(appId) {
    const r = mirrorGet('app_drafts', draftId(appId));
    if (!r || r.pending || !r.base?.data || r.base.deleted) return null;
    return r.base.rev ?? null;
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
        return draftContent(d.files[path]);
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
          if (!d) return null;
          const files = {};
          for (const [p, h] of Object.entries(d.files)) files[p] = h === null ? null : await draftContent(h);
          // rev = Revision der app_drafts-Zeile im Core (base_draft_rev für ctox.coding.turn, §16.44)
          return { base_app_ver: d.base_app_ver, files, manifest: d.manifest, updated_at: d.updated_at, rev: confirmedDraftRev(appId) };
        },
        // Text oder Bytes (Binärdateien wie PNG-Icons bleiben Bytes, Hash beim publish über die Bytes)
        write(path, content) {
          return draftHandle((async () => {
            requirePerm('apps.source.edit');
            if (typeof path !== 'string' || !path || content == null) throw new CtoxError('invalid', 'path und Inhalt nötig', { path: 'path' });
            noManifestFile(path);
            const hash = await storeDraftContent(appId, path, await artifactContent(content));
            return draftMutate(appId, (d) => { d.files[path] = hash; });
          })());
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
          const cur = await draftOf(appId);
          const hash = cur && Object.prototype.hasOwnProperty.call(cur.files, from) ? cur.files[from] : versionDoc(appId, viewBase(cur))?.manifest?.files?.[from];
          await draftMutate(appId, (d) => {
            d.files[to] = hash;
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
          const hash = await storeDraftContent(appId, SCHEMA_FILE, text);
          await draftMutate(appId, (d) => { d.files[SCHEMA_FILE] = hash; });
          const base = (await draftOf(appId))?.base_app_ver ?? null;
          return { base_app_ver: base, diff: classifyCollections(await baseSchemaOf(appId, base), parsed, { baseVer: base }) };
        },
        async discard() {
          requirePerm('apps.source.edit');
          await migrateLegacyDraft(appId);
          if (draftDoc(appId)) await localWrite(PLATFORM_APP, 'delete', 'app_drafts', draftId(appId), null).local;
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
            const files = {};
            for (const [p, h] of Object.entries(d.files)) files[p] = h === null ? null : await draftContent(h);
            const r = await callLeader('publishDraft', [appId, base ?? null, files, d.manifest ?? {}]);
            if (r.status !== 'rejected') await localWrite(PLATFORM_APP, 'delete', 'app_drafts', draftId(appId), null).local; // nur der eigene Entwurf
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
      if (!d || d.id==='ctox-platform' || isLib(d.id)) continue;
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
  // ---- §16.39 Nr. 1/2 (+ Ergänzung LIBS2), §16.37: Artefakt-URLs mit Instanz-Bindung ----
  // Eine App-Instanz = eine URL-Wurzel: /~a/<app>@<ver>~shell@<sv>~lib:<name>@<M.m>…/<pfad> (Libs nach Name sortiert).
  // Die Bindung (Shell-Version + je Bibliothek aus dem Shell-Manifest `libs` die Minor) entsteht synchron aus dem Spiegel:
  // artifacts.bind() beim Mount friert sie ein; artifacts.url() ohne Bindung nimmt immer die aktuell aktive Shell (K-1d).
  // Die Shell selbst: /~a/shell@<sv>~lib:…/…; Bibliotheken: /~a/lib:<name>@<M.m>[~lib:…]/… (immutable).
  const activeShellVer = () => activations.get('shell')?.activated_ver ?? visible(mirrorGet('business_apps', 'shell'))?.active_ver ?? null;
  const LIB_NAME = /^[a-z0-9][a-z0-9._-]*$/;
  const SEG = /^[A-Za-z0-9._+-]+$/;
  function libVersionsOf(name, major) {
    const out = [];
    for (const r of mirror.get('business_app_versions')?.values() ?? []) {
      const d = visible(r);
      const m = d && d.app_id === `${LIB_PREFIX}${name}` ? /^(\d+)\.(\d+)$/.exec(String(d.app_ver)) : null;
      if (m && m[1] === String(major)) out.push(Number(m[2]));
    }
    return out.sort((a, b) => a - b);
  }
  /** Höchste veröffentlichte Minor der Major (§16.37) als "<major>.<minor>" oder null. */
  function resolveLibMinor(name, major) {
    const minors = libVersionsOf(name, major);
    return minors.length ? `${major}.${minors.at(-1)}` : null;
  }
  async function setActiveLib(name, major, ver) {
    await db.tx(async (t) => {
      const key = `active:lib:${name}@${major}`;
      const cur = await t.get('meta', key);
      const minorOf = (v) => Number(String(v).split('.')[1] ?? -1);
      if (!cur || minorOf(cur.value) < minorOf(ver)) await t.put('meta', { key, value: ver });
    });
  }
  // Bindungen: libs des Shell-Manifests (der gebundenen Shell-Version) plus eigene libs der App; {name: 'M.m'}
  function resolveLibs(appId, ver, shellVer) {
    const bound = { ...(shellVer ? versionDoc('shell', shellVer)?.manifest?.libs ?? {} : {}), ...(versionDoc(appId, ver)?.manifest?.libs ?? {}) };
    const libs = {};
    for (const [name, majorRaw] of Object.entries(bound)) {
      const major = String(majorRaw).split('.')[0];
      const resolved = LIB_NAME.test(name) ? resolveLibMinor(name, major) : null;
      if (!resolved) continue; // nicht veröffentlicht → keine Bindung, ctox:lib/ lädt dann nicht (kein Rückgriff)
      libs[name] = resolved;
      track(setActiveLib(name, major, resolved).catch(() => {}));
    }
    return libs;
  }
  const libSuffix = (libs, skip = null) => Object.keys(libs).filter((n) => n !== skip).sort().map((n) => `~lib:${n}@${libs[n]}`).join('');
  function encPath(path) { return artifactUrl('x', '0', path, 'x').slice('/~a/x@0~shell@x/'.length); } // gleiche Pfadprüfung
  // K-1d / §16.39 „beim Mount der App-Instanz“: Bindung = {appId, ver, shellVer, libs, root}; NICHTS wird je app@ver gemerkt.
  // Gleiche Regeln wie der Stub (@68b83a3, artifacts-Block).
  function makeBinding(appId, ver, o = {}) {
    if (typeof appId !== 'string' || !appId || ver == null || ver === '') throw new CtoxError('invalid', 'appId und ver nötig', { path: 'appId' });
    ver = String(ver);
    if (!SEG.test(ver)) throw new CtoxError('invalid', 'Version mit unzulässigen Zeichen', { path: 'ver' });
    if (isLib(appId)) {
      const name = appId.slice(LIB_PREFIX.length);
      if (!LIB_NAME.test(name) || !/^\d+\.\d+$/.test(ver)) throw new CtoxError('invalid', 'lib-URL braucht lib:<name> und <major>.<minor>', { path: 'ver' });
      const libs = { ...(o.libs ?? {}) };
      return deepFreeze({ appId, ver, shellVer: null, libs, root: `/~a/${LIB_PREFIX}${name}@${ver}${libSuffix(libs, name)}/` });
    }
    const shellVer = appId === 'shell' ? ver : String(o.shellVer ?? activeShellVer() ?? '');
    if (!shellVer) throw new CtoxError('unavailable', 'Keine aktive Shell-Version für die Instanz-URL', { path: 'shellVer' });
    const libs = resolveLibs(appId, ver, shellVer);
    const root = appId === 'shell' ? `/~a/shell@${encodeURIComponent(ver)}${libSuffix(libs)}/` : `/~a/${encodeURIComponent(appId)}@${encodeURIComponent(ver)}~shell@${encodeURIComponent(shellVer)}${libSuffix(libs)}/`;
    return deepFreeze({ appId, ver, shellVer, libs, root });
  }
  /** §16.39 Nr. 1 (A6), synchron; ohne Bindung: aktuell aktive Shell (oder o.shellVer). */
  function artifactsUrl(appId, ver, path, o = {}) { return makeBinding(appId, ver, o).root + encPath(path); }
  /** §16.39 Bindung je MOUNT: einmal beim Mount, eingefroren; url(path) synchron. */
  function bindArtifacts(appId, ver, o = {}) {
    const b = makeBinding(appId, ver, o);
    // §16.50: Worker (Monaco u. a.) immer gleich-ursprünglich unter der gebundenen Lib-Wurzel, nie data:/blob:
    const workerUrl = (lib, path) => {
      if (typeof lib !== 'string' || !b.libs[lib]) throw new CtoxError('not_found', `Bibliothek ${lib} ist in dieser Bindung nicht gebunden`, { path: 'lib' });
      return `/~a/${LIB_PREFIX}${lib}@${b.libs[lib]}${libSuffix(b.libs, lib)}/${encPath(path)}`;
    };
    return Object.freeze({ ...b, url: (path) => b.root + encPath(path), workerUrl });
  }
  const contentBytes = (c) => (typeof c === 'string' ? new TextEncoder().encode(c).byteLength : c?.byteLength ?? 0);
  // SW-Miss (Vertrag v4/sw.js 9b414f663): genau diese eine Datei über Spur 4 holen, sha256 prüfen, in artifacts ablegen.
  const libInflight = new Map();
  async function libFetchLeader(lib, major, path, ver, sha = null) {
    // §16.39 LIBS2: genau die Minor aus der URL holen, nie die gerade aktive
    const v = typeof ver === 'string' && /^\d+\.\d+$/.test(ver) && ver.split('.')[0] === String(major) ? ver : null;
    if (!v) return { ok: false, reason: 'invalid' };
    const hash = versionDoc(`${LIB_PREFIX}${lib}`, v)?.manifest?.files?.[path];
    if (!hash) return { ok: false, reason: versionDoc(`${LIB_PREFIX}${lib}`, v) ? 'not_found' : 'manifest' };
    if (sha && sha !== hash) return { ok: false, reason: 'hash_mismatch' }; // §16.50: erwartete sha256 des SW passt nicht → Fehler statt Auslieferung
    await writeFilesMeta(`${LIB_PREFIX}${lib}`, v, versionDoc(`${LIB_PREFIX}${lib}`, v).manifest.files); // K-1d: Zuordnung der Bibliotheks-Minor
    if (!libInflight.has(hash)) {
      libInflight.set(hash, (async () => {
        const have = await db.get('artifacts', hash);
        if (have && (await sha256Hex(have.content)) === hash) return { ok: true, ver: v, hash };
        for (let attempt = 0; attempt < 2; attempt++) {
          const got = await transport.artifact(hash).then(artifactContent).catch(() => null);
          if (got == null) return { ok: false, reason: 'artifact_missing' };
          if ((await sha256Hex(got)) !== hash) { simRecord.refetches++; continue; }
          const now = Date.now();
          await db.tx((t) => t.put('artifacts', { hash, content: got, app_id: `${LIB_PREFIX}${lib}`, app_ver: v, path, size: contentBytes(got), stored_at: now, hit_at: now }));
          await setActiveLib(lib, String(major), v);
          await enforceLibBudget();
          return { ok: true, ver: v, hash };
        }
        return { ok: false, reason: 'hash_mismatch' };
      })().finally(() => libInflight.delete(hash)));
    }
    return libInflight.get(hash);
  }
  // LRU je Byte-Budget über die Bibliotheksdateien in artifacts (App-Release-Artefakte bleiben unberührt).
  async function enforceLibBudget() {
    const rows = (await db.all('artifacts')).filter((r) => isLib(r.app_id));
    let total = rows.reduce((a, r) => a + (r.size ?? contentBytes(r.content)), 0);
    if (total <= libByteBudget) return [];
    rows.sort((a, b) => (a.hit_at ?? a.stored_at ?? 0) - (b.hit_at ?? b.stored_at ?? 0));
    const evict = [];
    for (const r of rows) { if (total <= libByteBudget) break; evict.push(r.hash); total -= r.size ?? contentBytes(r.content); }
    if (evict.length) await db.tx(async (t) => { for (const h of evict) await t.del('artifacts', h); });
    return evict;
  }
  // SW meldet Treffer gebündelt (ctox:lib-hit, ≤ 1/s); der Kernel bündelt zusätzlich auf ≤ 1 Schreibvorgang je Sekunde.
  const pendingHits = new Map();
  let hitFlush = null;
  let lastHitFlush = 0;
  async function flushLibHits() {
    hitFlush = null; lastHitFlush = Date.now();
    const hits = [...pendingHits]; pendingHits.clear();
    if (!hits.length || closed) return;
    await db.tx(async (t) => {
      for (const [hash, at] of hits) {
        const r = await t.get('artifacts', hash);
        if (r && isLib(r.app_id) && (r.hit_at ?? 0) < at) await t.put('artifacts', { ...r, hit_at: at });
      }
    });
  }
  function noteLibHits(list) {
    for (const h of Array.isArray(list) ? list : []) {
      // SW-Vertrag LIBS2: {lib, major, ver, path, sha, n, last_ms}; sha fehlt → aus dem Manifest der Minor
      const hash = typeof h === 'string' ? h : h?.sha ?? (h?.lib && h?.ver ? versionDoc(`${LIB_PREFIX}${h.lib}`, h.ver)?.manifest?.files?.[h.path] : null);
      if (typeof hash === 'string' && hash) pendingHits.set(hash, Math.max(pendingHits.get(hash) ?? 0, Number(h?.last_ms ?? h?.at) || Date.now()));
    }
    if (!hitFlush && pendingHits.size) {
      const wait = Math.max(0, lastHitFlush + 1000 - Date.now());
      hitFlush = new Promise((r) => later(r, wait)).then(flushLibHits);
      track(hitFlush);
    }
    return hitFlush ?? Promise.resolve();
  }
  /** Nachrichten des Service Workers (ctox:lib-hit, ctox:lib-miss); Antwort auf lib-miss über den übergebenen Port. */
  // §16.50: Der Kernel bedient ctox:lib-miss unabhängig davon, welcher Client (Fenster, Worker, Client ohne Kernel) den
  // Miss im SW ausgelöst hat; Antwort an den SW mit der Datei (sha256-geprüft) oder einem Grund, spätestens vor dessen 15 s.
  const LIB_MISS_ANSWER_MS = opts.libMissAnswerMs ?? 14000;
  async function onSwMessage(data, port) {
    if (closed || !data || typeof data !== 'object') return;
    if (data.type === 'ctox:lib-hit') { await noteLibHits(data.hits ?? data.shas); return; }
    if (data.type !== 'ctox:lib-miss') return;
    let timer;
    const timeout = new Promise((r) => { timer = setTimeout(() => r({ ok: false, reason: 'timeout' }), LIB_MISS_ANSWER_MS); });
    let r;
    try {
      r = await Promise.race([callLeader('libFetch', [String(data.lib), String(data.major), String(data.path), data.ver ?? null, typeof data.sha === 'string' ? data.sha : null]), timeout]);
    } catch (e) { r = { ok: false, reason: e?.code ?? 'unavailable' }; } finally { clearTimeout(timer); }
    let msg = { type: 'ctox:lib-abort', reason: r.reason, lib: data.lib, ver: data.ver ?? null, path: data.path }, transfer = [];
    if (r.ok) {
      const row = await db.get('artifacts', r.hash);
      const bytes = row ? (typeof row.content === 'string' ? new TextEncoder().encode(row.content) : new Uint8Array(row.content)) : null;
      if (bytes && (await sha256Hex(bytes)) === r.hash) {
        const body = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
        msg = { type: 'ctox:lib-ready', lib: data.lib, ver: r.ver, path: data.path, sha: r.hash, mime: artifactMime(data.path), size: bytes.byteLength, body };
        transfer = [body];
      } else msg = { ...msg, reason: 'hash_mismatch' };
    }
    try { port?.postMessage(msg, transfer); } catch { /* Port zu */ }
    return { ...r, reply: msg.type };
  }
  const swMessageListener = (ev) => { void onSwMessage(ev.data, ev.ports?.[0]); };
  try { globalThis.navigator?.serviceWorker?.addEventListener?.('message', swMessageListener); } catch { /* kein SW */ }
  let platformApi = null;
  function platformCommand(type, payload) {
    guardAlive();
    platformApi ??= makeAppApi(PLATFORM_APP_ID, '1');
    return platformApi.command(type, payload);
  }
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
    // K-1c Punkt 7 / §16.29 B, §16.33: App-Store-Wege als Plattformbefehle (Vergleich-und-Setzen auf base_app_ver).
    // Ergebnis wie api.command: {id, status (live), local, result}; Ablehnung im write_result mit exaktem Code
    // stale_base | migration_required | locally_modified | forbidden | not_found.
    publishApp: (appId, o = {}) => platformCommand('ctox.app.publish', { app_id: appId, base_app_ver: o.base_app_ver ?? null, draft_id: o.draft_id ?? `${userId}/${appId}` }),
    updateApp: (appId, o = {}) => platformCommand('ctox.app.update', { app_id: appId, base_app_ver: o.base_app_ver ?? null, release_ver: o.release_ver, ...(o.discard_local ? { discard_local: true } : {}) }),
    rollbackApp: (appId, o = {}) => platformCommand('ctox.app.rollback', { app_id: appId, base_app_ver: o.base_app_ver ?? null, target_ver: o.target_ver }),
    releaseApp: (appId, o = {}) => platformCommand('ctox.app.release', { app_id: appId, base_app_ver: o.base_app_ver ?? null, channel: o.channel ?? 'stable' }),
    removeApp: (appId, o = {}) => platformCommand('ctox.app.remove', { app_id: appId, base_app_ver: o.base_app_ver ?? null }),
    artifacts: Object.freeze({
      async read(hash) {
        guardAlive();
        const a = await db.get('artifacts', hash);
        if (!a) throw new CtoxError('not_found', `Artefakt ${hash} nicht lokal`);
        return new Blob([a.content], { type: artifactMime(a.path) }); // Bytes unverändert, Typ nach Endung (PNG → image/png)
      },
      async manifest(appId, ver) { return versionDoc(appId, ver)?.manifest ?? null; },
      /** §16.39 Nr. 1 (A6): /~a/<app>@<ver>~shell@<sv>/<path>; sv = opts.shellVer, sonst beim Mount fixierte Shell-Version. */
      url: (appId, ver, path, o) => { guardAlive(); return artifactsUrl(appId, ver, path, o); },
      /** §16.39 Bindung je MOUNT (K-1d): einmal beim Mount aufrufen → {appId, ver, shellVer, libs, root, url(path)}. */
      bind: (appId, ver, o) => { guardAlive(); return bindArtifacts(appId, ver, o); },
    }),
    /** §16.39 Nr. 2: SW-Nachrichten (ctox:lib-hit, ctox:lib-miss) – im Browser automatisch an navigator.serviceWorker gebunden. */
    libs: Object.freeze({ onSwMessage, resolve: resolveLibMinor, enforceBudget: () => enforceLibBudget(), get byteBudget() { return libByteBudget; } }),
    source: sourceApi,
    identity: Object.freeze({
      get instance_id() { return instanceId; },
      get user_id() { return userId; },
      /** §16.43: eigene device_pairing_id („Dieses Gerät“) aus der Gerätebindung des Bootstraps; Fake-Core deterministisch. */
      get device_id() { return deviceId(); },
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
        state: makeLive([tag], () => ({...win,size:{...win.size}})),
        setSize(size) { win.size = { width: Number(size?.width ?? 0), height: Number(size?.height ?? 0) }; notify(tag); },
        setMode(mode) { win.mode = String(mode); notify(tag); },
        reopen() { win.closed = false; notify(tag); },
      });
    },

  });

  // ---- Sim-Implementierung (läuft im Leiter) ----
  const simImpl = {
    push: (collection, docs) => corePush(collection, docs),
    setLink(partial) {
      if(partial.state!==undefined && !['online','offline'].includes(partial.state))throw new CtoxError('invalid','Link state must be online or offline');
      link = { ...link, ...partial };
      if(fakeCore)fakeCore.online = link.state === 'online';
      netChanged();
      if (link.state === 'online') { track(deliver().then(() => schedulePump())); }
      return { ...link };
    },
    setBacklog(n) { extraBacklog = Math.max(0, Number(n) || 0); netChanged(); },
    rejectNext(reason) { fakeCore?.reject.push(reason); },
    rejectNextUpload(reason) { fakeCore?.rejectUpload.push(reason); },
    migrateNext(o) { fakeCore?.migrate.push(o ?? {}); },
    failCommand(type, error) { fakeCore?.failures.set(type, error); },
    async publishAppVersion(appId, ver, o) {
      await publishInternal(appId, ver, o);
      if (isLib(appId)) { await waitUntil(() => versionDoc(appId, ver) || closed); return null; } // Bibliotheken werden nie aktiviert
      await waitUntil(() => isTerminalFor(appId, ver) || visible(mirrorGet('business_apps', appId))?.active_ver !== ver || closed);
      return activations.has(appId) ? { ...activations.get(appId) } : null;
    },
    // §16.22 A2/A3/A6/A9 + §16.26 + Befund B4. Im Leiter, weil der Test-Core hier die Veröffentlichung simuliert.
    publishDraft(appId, base, draftFiles, draftManifest) {
      return withPublishLock(appId, async () => {
        const value = await simImpl.publishDraftLocked(appId, base, draftFiles, draftManifest);
        return { value, hold: value.status === 'accepted' ? settleActivation(appId, value.rev) : null };
      });
    },
    async publishDraftLocked(appId, base, draftFiles, draftManifest) {
      const reject = (reason, detail) => ({ status: 'rejected', reason, ...(detail ? { detail } : {}) });
      const active = activeVerOf(appId);
      if ((active ?? null) !== (base ?? null)) {
        return reject('stale_base', base == null ? `App ${appId} existiert bereits (aktiv: ${active})` : `Basis ${base} ist nicht mehr aktiv (aktiv: ${active ?? '–'})`);
      }
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
        await publishCore(appId, ver, {
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
    libFetch(lib, major, path, ver, sha) { return libFetchLeader(lib, major, path, ver, sha); },
    // §16.45 „Neu verbinden“: voller Neuaufbau des Transports im Leiter (nie Seiten-Reload); alle Bereiche neu abonniert
    async reconnect() {
      guardAlive();
      for (const e of collScopes.values()) { e.complete = false; e.error = null; }
      link = { ...link, state: 'offline' }; inflight.clear(); inflightOps.clear(); netChanged();
      if (typeof transport.reconnect === 'function') {
        await transport.reconnect();
        for (const c of new Set(['business_apps', 'business_app_versions', 'business_commands', 'files', 'shell_preferences', 'app_user_preferences', 'app_drafts', ...mirror.keys()])) await subscribeCollection(c);
        await deliver(); schedulePump();
      } else {
        try { transport.close(); } catch { /* egal */ }
        transportConnected = true;
        await connectTransport();
      }
      refreshCollections();
      return { link: { ...link }, collections: cloneValue(collectionsView) };
    },
    async reportFailed(appId, ver, reason) {
      const cur = activations.get(appId) ?? {};
      const good = (cur.good_vers ?? []).filter((v) => v !== ver);
      const lastGood = cur.activated_ver && cur.activated_ver !== ver ? cur.activated_ver : good.at(-1) ?? null;
      const v = await setActivation(appId, { target_ver: ver, activated_ver: lastGood, good_vers: good, state: 'failed', error: `reported: ${reason}` });
      return { ...v };
    },
    corruptNextFetch(n){corruptNextFetch+=n;},

  };
  const forward = (name) => (...args) => Promise.resolve().then(()=>callLeader(name,args));

  // ---- Sim-Fassade (Test-Hooks) ----
  const sim = {
    scopeGrants:()=>cloneValue(fakeCore?.grantMessages??[]),
    failAppGrant:appId=>fakeCore?.deniedAppGrants.add(appId),
    rejectNext: (reason = 'simulated_rejection') => forward('rejectNext')(reason),
    rejectNextUpload: (reason = {code:'forbidden',reason:'Upload denied'}) => forward('rejectNextUpload')(reason),
    migrateNext: (o = {}) => forward('migrateNext')({ dropFields: o.dropFields ?? [] }),
    failCommand: (type, error = 'simulated failure') => forward('failCommand')(type, error),
    onCommand(type, fn) { fakeCore?.handlers.set(type, fn); },
    push: (collection, docs) => forward('push')(collection, docs),
    setLink: (partial) => forward('setLink')(partial),
    setBacklog: (n) => forward('setBacklog')(n),
    publishAppVersion(appId,ver,o={}){
      if(o.module!==undefined)return Promise.reject(new CtoxError('invalid','App modules belong to Shell/testkit'));
      if(isLeader||!bc)return Promise.resolve().then(()=>simImpl.publishAppVersion(appId,ver,o));
      return callLeader('publishAppVersion',[appId,ver,o]).then(()=>(isLib(appId)?null:activations.get(appId)??null));
    },
    corruptNextFetch:(n=1)=>callLeader('corruptNextFetch',[n]),
    retireGenerationsNow:appId=>{const a=activations.get(appId);return retireGenerations(appId,a?.activated_ver,a?.previous_ver);},
    // Loader-Rolle (§16.15): Shell starten und live tauschen.
    async startShell(shellModule, o = {}) {
      if (typeof shellModule?.startShell !== 'function') throw new CtoxError('invalid', 'Shell-Modul exportiert kein startShell()');
      await ready;
      await withTimeout(Promise.resolve().then(()=>shellModule.startShell(kernel,{restored:o.restored})),remountTimeoutMs,
        ()=>{try{shellModule.stopShell?.();}catch{}});
      guardAlive();

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
      await windowStateApi.put('shell',snap??null);
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
    artifactRows: async () => db.all('artifacts'),
    /** Fake-Core: Zeilen einer Collection, wie der Core sie hält (auch nicht abonnierte wie _audit). */
    coreRows: async (collection) => (fakeCore ? [...fakeCore.core.docs.values()].filter((r) => r.collection === collection && !r.deleted && r.data).map((r) => cloneValue(r.data)) : []),
    /** Fake-Harness: Ergebnis der nächsten Agentenläufe ({status:'done'|'failed', result}). */
    agentOutcome: (fn) => fakeCore?.runtime.setAgentOutcome(fn),
    /** Fake-Core: Write eines anderen Clients direkt an den Core (z. B. doppelte Befehls-id). */
    coreWrite: (entry) => fakeCore?.write(entry),
    /** Fake-Core: Plattform-Allowlist für process.run (CAP-3d; Name → Pfad oder {program|interpreter+script, env}). */
    setProcessAllowlist: (map) => { if (fakeCore) fakeCore.core.processAllowlist = map ? cloneValue(map) : null; },
    coreLog: () => [...(fakeCore?.runtime.log ?? [])],
    /** Fake-Core: wie oft sich der Transport verbunden hat (kernel.reconnect = voller Neuaufbau). */
    get transportConnects() { return fakeCore?.connects ?? 0; },
    inspectMeta: async (key) => db.get('meta', key),
    /** @deprecated seit §16.19 – Alias für `windowStateRows`. */
    windowState: async () => db.all('window_state'),
    windowStateRows:async()=>db.all('window_state'),
    get refetches(){return simRecord.refetches;},
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
    for (const t of timers) clearTimeout(t);
    timers.clear();
    clearInterval(ticker);
    clearInterval(reconcileTimer);
    if (typeof document !== 'undefined') document.removeEventListener('visibilitychange', onVis);
    if (typeof window !== 'undefined') window.removeEventListener('focus', onVis);
    lockAbort?.abort();
    const rel = releaseLock; releaseLock = null; rel?.();
    isLeader = false;
    sendLeaderHint(); // Schließen ist ein Leiterverlust (§16.50 Nr. 3)
    try { swContainer?.removeEventListener?.('controllerchange', onControllerChange); } catch { /* kein SW */ }
    for (const l of [...allLives]) l.dispose();
    allLives.clear();
    activationListeners.clear();
    routeListeners.clear();
    for (const w of waitList.splice(0)) w.resolve();
    for (const w of rpcWaiters.values()) w.rej(new CtoxError('unavailable', 'Kernel geschlossen'));
    rpcWaiters.clear();
    try { globalThis.navigator?.serviceWorker?.removeEventListener?.('message', swMessageListener); } catch { /* kein SW */ }
    try { bc?.close(); } catch { /* egal */ }
    bc = null;
    try { transport.close(); } catch {}
    await db?.close();
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
    apps:shellApi.apps,activation:shellApi.activation,metrics:shellApi.metrics,leader:shellApi.leader,status:statusZone,windowState:windowStateApi,artifacts:shellApi.artifacts,source:shellApi.source,identity:shellApi.identity,
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
    libs: shellApi.libs,
    source: shellApi.source,
    identity: shellApi.identity,
    /** §16.45: voller Neuaufbau des Transports (nie Seiten-Reload); im Leser über den Leiter. */
    reconnect: () => { guardAlive(); return callLeader('reconnect', []); },
    shellApi: () => shellApi,
    sim,
    receivePage,
    loadModule,
    async evict(collection) {
      guardAlive();
      const records = await evictCollection(db,collection,assertGen);
      for(const row of records)mirrorPut(row);
      evicted.add(collection);scopeCursors.delete(collection);subscriptions.delete(collection);untrackCollection(collection);
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
    // E48.1: only the fake core owns a built-in catalog. Production consumes Core metadata.
    const builtinPlatform = fakeCore ? await db.tx(async t=>{
      const existing = await t.get('docs',['business_apps','ctox-platform']);
      if (existing) return [];
      const rows = [
        ['business_app_versions',{id:'ctox-platform@1',app_id:'ctox-platform',app_ver:'1',manifest:PLATFORM_MANIFEST,builtin:true}],
        ['business_apps',{id:'ctox-platform',active_ver:'1'}],
      ];
      const result=[];
      for (const [collection,data] of rows) {
        const base={data,deleted:false,rev:null,seq:'0'};
        const row={collection,id:data.id,...base,pending:false,base};
        await putDoc(t,row);result.push(row);
      }
      return result;
    }) : await db.tx(async t=>{
      // Retire bootstrap rows left by older clients, but preserve every native catalog row.
      for(const row of await t.all('docs'))if(row.collection==='business_app_versions'&&row.data?.app_id===PLATFORM_APP_ID&&row.data?.builtin===true&&row.rev==null&&row.seq==='0'&&!row.pending){
        await t.del('docs',[row.collection,row.id]);
        const active=await t.get('docs',['business_apps',PLATFORM_APP_ID]);
        if(active?.data?.active_ver===row.data.app_ver&&active.rev==null&&row.seq==='0')await t.del('docs',['business_apps',PLATFORM_APP_ID]);
        const activation=await t.get('meta','activation:'+PLATFORM_APP_ID);
        if(activation?.activated_ver===row.data.app_ver)await t.del('meta','activation:'+PLATFORM_APP_ID);
      }
      return [];
    });
    if (fakeCore) for (const row of builtinPlatform) if (!fakeCore.core.docs.has(fakeCore.key(row.collection,row.id))) await fakeCore.push(row.collection,[row.data]);
    await loadFromStorage();
    if (hasLocks) {
      bc = new BroadcastChannel(channelName);
      bc.onmessage = (ev) => { onMessage(ev.data).catch((e) => { if (!closed) console.error('[ctox-kernel] message', e); }); };
    }
    await elect();
    guardAlive();
    swHintStarted = true;
    sendLeaderHint(true); // §16.50 Nr. 3: Startmeldung, auch für Leser (leader:false)
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
