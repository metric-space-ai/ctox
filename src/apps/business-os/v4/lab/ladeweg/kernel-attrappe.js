// Kernel-Attrappe fuer das Ladeweg-Labor (Shell v4, Strang P1).
// Ersetzt den echten Kernel (Sync-v4 §10.2a Nr. 6/8) nur so weit, wie der Ladeweg ihn braucht:
// Artefakte inhaltsadressiert speichern, Dateilisten ablegen, Aktivierungszeiger setzen. IDB-Layout wie der Kernel
// (K-1d): artifacts {hash, content: string | Uint8Array}, meta {key: 'files:<app>@<ver>', value: {pfad: sha256}};
// der Aktivierungszeiger meta {key: 'active:<app>', value} ist nur Labor-Zustand (der SW liest ihn nie).
// In Produktion kommen die Dateien ueber CTOX Sync (WebRTC, Spur 4); im Labor reicht sie das
// Messskript direkt in die Seite (kein HTTP fuer App-Code).

export const DB_NAME = 'ctox-v4-lab';
const DB_VERSION = 2; // 2 = Kernel-Layout (K-1d); 1 = altes Labor-Schema (manifests, artifacts {bytes})

let dbPromise = null;
export function openKernelDb() {
  if (dbPromise) return dbPromise;
  dbPromise = new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      for (const name of [...db.objectStoreNames]) db.deleteObjectStore(name);
      db.createObjectStore('artifacts', { keyPath: 'hash' });
      db.createObjectStore('meta', { keyPath: 'key' });
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => { dbPromise = null; reject(req.error); };
  });
  return dbPromise;
}

export async function sha256Hex(bytes) {
  const d = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
  let s = '';
  for (const b of d) s += b.toString(16).padStart(2, '0');
  return s;
}

// Inhalt wie im Kernel: Text bleibt Zeichenkette (sha256 ueber UTF-8), Binaerdaten als Uint8Array.
function toContent(content) {
  if (content instanceof ArrayBuffer) return new Uint8Array(content.slice(0));
  if (ArrayBuffer.isView(content)) return new Uint8Array(content.buffer.slice(content.byteOffset, content.byteOffset + content.byteLength));
  return String(content);
}
const bytesOf = (c) => (typeof c === 'string' ? new TextEncoder().encode(c) : c);

function tx(db, stores, mode, fn) {
  return new Promise((resolve, reject) => {
    const t = db.transaction(stores, mode);
    const out = fn(t);
    t.oncomplete = () => resolve(out);
    t.onerror = () => reject(t.error);
    t.onabort = () => reject(t.error || new Error('aborted'));
  });
}

function metaGet(db, key) {
  return new Promise((resolve, reject) => {
    const r = db.transaction('meta').objectStore('meta').get(key);
    r.onsuccess = () => resolve(r.result ? r.result.value : null);
    r.onerror = () => reject(r.error);
  });
}

/**
 * Installiert eine App-Version wie die Aktivierung im Kernel: alle Dateien als Artefakte {hash, content}, dann
 * meta files:<app>@<ver>. files: { pfad: string | ArrayBuffer | Uint8Array }  ->  {app_id, app_ver, files: {pfad: sha256}}
 */
export async function installVersion({ app_id, app_ver, files }) {
  const db = await openKernelDb();
  const entries = await Promise.all(Object.entries(files).map(async ([path, raw]) => {
    const content = toContent(raw);
    return [path, content, await sha256Hex(bytesOf(content))];
  }));
  const list = Object.fromEntries(entries.map(([p, , h]) => [p, h]));
  await tx(db, ['artifacts', 'meta'], 'readwrite', (t) => {
    const a = t.objectStore('artifacts');
    for (const [, content, hash] of entries) a.put({ hash, content });
    t.objectStore('meta').put({ key: `files:${app_id}@${app_ver}`, value: list });
  });
  return { app_id, app_ver, files: list };
}

export async function setActive(app_id, app_ver) {
  const db = await openKernelDb();
  await tx(db, ['meta'], 'readwrite', (t) => t.objectStore('meta').put({ key: `active:${app_id}`, value: app_ver }));
}

export async function getActive(app_id) {
  return metaGet(await openKernelDb(), `active:${app_id}`);
}

/** Dateiliste {pfad: sha256} (meta files:<app>@<ver>) oder null – Kernel-Methode readFiles des Blob-Ladewegs. */
export async function readFiles(app_id, app_ver) {
  return metaGet(await openKernelDb(), `files:${app_id}@${app_ver}`);
}

/** artifacts[sha].content (string | Uint8Array) oder null. */
export async function readArtifact(sha) {
  const db = await openKernelDb();
  return new Promise((resolve, reject) => {
    const r = db.transaction('artifacts').objectStore('artifacts').get(sha);
    r.onsuccess = () => resolve(r.result ? r.result.content : null);
    r.onerror = () => reject(r.error);
  });
}

/** Nur fuer Szenario (e): ueberschreibt den Inhalt hinter einem Hash (simuliert Manipulation im Speicher). */
export async function tamperArtifact(sha, content) {
  const db = await openKernelDb();
  await tx(db, ['artifacts'], 'readwrite', (t) => t.objectStore('artifacts').put({ hash: sha, content: toContent(content) }));
}

// ---- Standardbibliotheken (§16.37/§16.39) fuer das Monaco-Labor (monaco.mjs): der Kernel legt die Dateiliste einer Minor
// (meta files:lib:<name>@<M.m>, sha256 aus dem Lib-Manifest) und je Anforderung genau eine Datei ab (Spur 4 simuliert).
export async function putFileList(app_id, app_ver, list) {
  const db = await openKernelDb();
  await tx(db, ['meta'], 'readwrite', (t) => t.objectStore('meta').put({ key: `files:${app_id}@${app_ver}`, value: list }));
}
export async function putArtifact(content) {
  const c = toContent(content);
  const hash = await sha256Hex(bytesOf(c));
  const db = await openKernelDb();
  await tx(db, ['artifacts'], 'readwrite', (t) => t.objectStore('artifacts').put({ hash, content: c }));
  return hash;
}
