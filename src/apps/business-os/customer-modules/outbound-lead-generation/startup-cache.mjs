// Last list state of this browser, shown at once while CTOX answers.
//
// The first authoritative read of the lead list pages ~850 leads through the
// native query channel. On thesen (10.10.2026) the list became visible after
// 25-55 s in a fresh headless browser and after about two minutes in the
// owner's Safari; the app showed only skeleton rows meanwhile. The rows kept
// here are the same compact list DTOs the app renders (never full leads,
// evidence or write inputs). They are display-only: mutating actions stay
// locked until the live read has replaced them, and every lead action still
// hydrates the full record from CTOX first.
const DB_NAME = 'ctox-outbound-startup-cache';
const STORE = 'snapshots';
const VERSION = 1;
const SNAPSHOT_SCHEMA = 1;
// A snapshot older than this is not worth showing: campaigns move quickly
// during research runs, and a week-old list would mislead more than help.
export const STARTUP_CACHE_MAX_AGE_MS = 7 * 24 * 60 * 60 * 1000;
const OPEN_TIMEOUT_MS = 1500;

function openDb() {
  return new Promise((resolve, reject) => {
    const idb = globalThis.indexedDB;
    if (!idb) return reject(new Error('indexedDB unavailable'));
    const timer = globalThis.setTimeout(() => reject(new Error('startup cache open timeout')), OPEN_TIMEOUT_MS);
    let request;
    try {
      request = idb.open(DB_NAME, VERSION);
    } catch (error) {
      globalThis.clearTimeout(timer);
      return reject(error);
    }
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE)) db.createObjectStore(STORE);
    };
    request.onsuccess = () => { globalThis.clearTimeout(timer); resolve(request.result); };
    request.onerror = () => { globalThis.clearTimeout(timer); reject(request.error); };
  });
}

function run(db, mode, work) {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE, mode);
    const request = work(tx.objectStore(STORE));
    tx.oncomplete = () => resolve(request?.result);
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
}

export function startupCacheScope({ host = '', userId = '' } = {}) {
  const user = String(userId || '').trim();
  if (!user) return '';
  return `${String(host || '').trim()}|${user}`;
}

export function isUsableStartupSnapshot(snapshot, now = Date.now()) {
  if (!snapshot || snapshot.schema !== SNAPSHOT_SCHEMA) return false;
  if (!Number.isFinite(snapshot.savedAtMs) || now - snapshot.savedAtMs > STARTUP_CACHE_MAX_AGE_MS) return false;
  return Array.isArray(snapshot.leads) && snapshot.leads.length > 0
    && Array.isArray(snapshot.imports) && Array.isArray(snapshot.sources)
    && Array.isArray(snapshot.adapters) && Array.isArray(snapshot.researchPolicies);
}

export async function readStartupSnapshot(scope) {
  if (!scope) return null;
  let db = null;
  try {
    db = await openDb();
    const snapshot = await run(db, 'readonly', (store) => store.get(scope));
    return isUsableStartupSnapshot(snapshot) ? snapshot : null;
  } catch {
    return null;
  } finally {
    try { db?.close(); } catch {}
  }
}

export function buildStartupSnapshot({ sources, adapters, imports, researchPolicies, leads }, now = Date.now()) {
  return {
    schema: SNAPSHOT_SCHEMA,
    savedAtMs: now,
    sources: Array.isArray(sources) ? sources : [],
    adapters: Array.isArray(adapters) ? adapters : [],
    imports: Array.isArray(imports) ? imports : [],
    researchPolicies: (Array.isArray(researchPolicies) ? researchPolicies : []).filter(Boolean),
    leads: Array.isArray(leads) ? leads : [],
  };
}

export async function writeStartupSnapshot(scope, snapshot) {
  if (!scope || !isUsableStartupSnapshot(snapshot)) return false;
  let db = null;
  try {
    db = await openDb();
    // Structured clone drops anything that is not plain data (RxDB handles,
    // functions); JSON round trip keeps the stored rows identical to what the
    // list renders.
    await run(db, 'readwrite', (store) => store.put(JSON.parse(JSON.stringify(snapshot)), scope));
    return true;
  } catch {
    return false;
  } finally {
    try { db?.close(); } catch {}
  }
}

// Actions that only change what this browser shows. Everything else writes to
// CTOX or starts work there and must wait for the live data.
const DISPLAY_ONLY_ACTIONS = new Set([
  'select-lead', 'select-campaign', 'select-detail-tab', 'select-contact-tab',
  'view-mode', 'lead-sort', 'lead-sort-dir', 'lead-filter-reset', 'lead-status-chip',
  'lead-tray-toggle', 'toggle-visible-leads', 'toggle-lead', 'clear-selection',
  'open-sources', 'close-sources', 'source-view', 'view-adapter-script',
  'close-adapter-script', 'close-lead-editor', 'close-sellify-import',
  'digest-preview-close', 'retry-sync',
]);

export function actionAllowedOnStartupSnapshot(action) {
  return DISPLAY_ONLY_ACTIONS.has(String(action || ''));
}
