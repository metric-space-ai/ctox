import { migrateV3Upgrade } from './migrate-v3.mjs';
import { CtoxError, cloneValue } from './values.mjs';
export const IDB_VERSION = 5;
export function storageError(error) {
  return error?.name === 'QuotaExceededError' ? new CtoxError('quota', 'Local storage quota exceeded') : error;
}
export const STORES = {
  docs: ['collection', 'id'],
  meta: 'key',
  outbox: 'op_id',
  artifacts: 'hash',
  archive_local: 'ref',
  window_state: 'key',
  blobs: 'ref', // rebuildable file cache; pending uploads stay pinned
};
export const STORE_NAMES = Object.keys(STORES);
const MEMORY_DBS = new Map();
const memKey = (k) => (Array.isArray(k) ? JSON.stringify(k) : String(k));
function keyOfValue(store, v) {
  const kp = STORES[store];
  return Array.isArray(kp) ? memKey(kp.map((p) => v[p])) : memKey(v[kp]);
}
export class MemoryBackend {
  constructor(name) {
    this.name = name;
    this.stores = new Map(STORE_NAMES.map((s) => [s, new Map()]));
    this.lock = Promise.resolve();
    this.kind = 'memory';
  }
  _ops(stores = this.stores) {
    const st = (s) => {
      const m = stores.get(s);
      if (!m) throw new Error(`Unbekannter Store ${s}`);
      return m;
    };
    return {
      get: async (s, k) => cloneValue(st(s).get(memKey(k))),
      put: async (s, v) => { st(s).set(keyOfValue(s, v), cloneValue(v)); },
      del: async (s, k) => { st(s).delete(memKey(k)); },
      all: async (s) => [...st(s).values()].map(cloneValue),
      outboxFor: async (ns, id) => [...st('outbox').values()].filter((e) => e.collection === ns && e.id === id).map(cloneValue),
    };
  }
  async outboxStat() {
    let count = 0, oldest = null;
    for (const e of this.stores.get('outbox').values()) { count++; if (oldest === null || e.created_at < oldest) oldest = e.created_at; }
    return { count, oldest_at: oldest };
  }
  tx(fn) {
    const run = this.lock.then(async () => {
      // _ops detaches values on get/all/put; map copies preserve rollback.
      // Retain immutable rows instead of cloning the entire database per write.
      const next = new Map([...this.stores].map(([s, values]) => [s, new Map(values)]));
      const result = await fn(this._ops(next));
      this.stores = next;
      return result;
    });
    this.lock = run.catch(() => {});
    return run;
  }
  read(fn) { return this.lock.then(() => fn(this._ops())); }
  get(s, k) { return this.read(t => t.get(s,k)); }
  all(s) { return this.read(t => t.all(s)); }
  close() {}
}
export class IdbBackend {
  static open(name) {
    return new Promise((resolve, reject) => {
      const req = indexedDB.open(name, IDB_VERSION);
      req.onupgradeneeded = (event) => {
        const db = req.result;
        for (const [s, kp] of Object.entries(STORES)) {
          if (db.objectStoreNames.contains(s)) continue;
          const os = db.createObjectStore(s, { keyPath: kp });
          if (s === 'outbox') os.createIndex('by_doc', ['collection', 'id']);
        }
        migrateV3Upgrade(db, req.transaction, event.oldVersion);
      };
      let blocked = false;
      req.onsuccess = () => { if (blocked) req.result.close(); else resolve(new IdbBackend(req.result, name)); };
      req.onerror = () => reject(req.error);
      req.onblocked = () => { blocked = true; reject(new CtoxError('unavailable', 'IndexedDB upgrade blocked: close the V3 tab')); };
    });
  }
  constructor(db, name) {
    this.db = db;
    this.name = name;
    this.kind = 'indexeddb';
    this.transactions = new Map();
    db.onversionchange = () => { this.onVersionChange?.(); db.close(); };
  }
  trackTransaction(t) {
    let done;const settled=new Promise(resolve=>{done=resolve;});this.transactions.set(t,settled);
    const finish=()=>{this.transactions.delete(t);done();};
    t.addEventListener('complete',finish,{once:true});t.addEventListener('abort',finish,{once:true});return t;
  }
  _wrap(t) {
    const req = (r) => new Promise((res, rej) => { r.onsuccess = () => res(r.result); r.onerror = () => rej(r.error); });
    return {
      get: (s, k) => req(t.objectStore(s).get(k)),
      put: (s, v) => req(t.objectStore(s).put(v)),
      del: (s, k) => req(t.objectStore(s).delete(k)),
      all: (s) => req(t.objectStore(s).getAll()),
      outboxFor: (ns, id) => req(t.objectStore('outbox').index('by_doc').getAll([ns, id])),
    };
  }
  async outboxStat() {
    const all = await this.all('outbox');
    return { count: all.length, oldest_at: all.length ? Math.min(...all.map((e) => e.created_at)) : null };
  }
  read(fn) { return this.transaction(fn, 'readonly'); }
  async tx(fn) {
    try { return await this.transaction(fn, 'readwrite'); }
    catch (error) {
      const failure=storageError(error);
      if(failure?.code!=='quota' || !this.recoverQuota)throw failure;
      // One bounded reclaim/retry; reclamation uses transaction() directly so
      // a full disk cannot recurse through the same quota handler.
      this.reclaiming ??= Promise.resolve().then(()=>this.recoverQuota()).finally(()=>{this.reclaiming=null;});
      if(!await this.reclaiming)throw failure;
      try { return await this.transaction(fn,'readwrite'); }
      catch(retryError){throw storageError(retryError);}
    }
  }
  transaction(fn, mode) {
    return new Promise((resolve, reject) => {
      let t;
      try { t = this.trackTransaction(this.db.transaction(STORE_NAMES, mode)); } catch (e) { reject(e); return; }
      let out, failed = null;
      t.oncomplete = () => (failed ? reject(failed) : resolve(out));
      t.onabort = () => reject(failed || t.error || new Error('IDB-Transaktion abgebrochen'));
      Promise.resolve().then(() => fn(this._wrap(t))).then(
        (v) => { out = v; },
        (e) => { failed = e; try { t.abort(); } catch { reject(e); } },
      );
    });
  }
  _ro(s, op) {
    return new Promise((resolve, reject) => {
      const t = this.trackTransaction(this.db.transaction([s], 'readonly'));
      const r = op(t.objectStore(s));
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => reject(r.error);
    });
  }
  get(s, k) { return this._ro(s, (os) => os.get(k)); }
  all(s) { return this._ro(s, (os) => os.getAll()); }
  async close() { this.db.close();await Promise.allSettled([...this.transactions.values()]); }
}
export async function openBackend(name, mode) {
  const useIdb = mode === 'indexeddb' || (mode !== 'memory' && typeof indexedDB !== 'undefined');
  if (useIdb) return IdbBackend.open(name);
  let db = MEMORY_DBS.get(name);
  if (!db) { db = new MemoryBackend(name); MEMORY_DBS.set(name, db); }
  return db;
}
export async function deleteBackend(name, kind) {
  if (kind === 'memory') { MEMORY_DBS.delete(name); return; }
  await new Promise((res,rej) => { const r = indexedDB.deleteDatabase(name); r.onsuccess=()=>res();r.onerror=()=>rej(storageError(r.error));r.onblocked=()=>rej(new CtoxError('unavailable','Close other identity tabs before deleting local data')); });
}
