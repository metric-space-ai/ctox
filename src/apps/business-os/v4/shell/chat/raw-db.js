// Shell v4 – Business-Chat (CHAT4): RxDB-förmige Fassade `db.raw.<collection>` für shared/business-chat.js.
//
// business-chat.js liest und schreibt ausschließlich über `db.raw.<name>` (RxDB-Collection der V2-Shell). v2-compat
// liefert nur `db.collection(name)` (lesen, Zeilen ohne toJSON, `$` je Zeile). Diese Fassade bildet genau die Formen ab,
// die business-chat.js benutzt (Inventar CHAT4.md), und nichts darüber hinaus:
//   find(q).exec()            → compat.state.db.collection(name).find(q).exec(), Zeilen als Dokumente (toJSON, …)
//   find(q).$.subscribe(cb)   → api.query(name, q) (LiveQuery, sofortige erste Lieferung wie RxQuery.$)
//   findOne(id).exec()        → compat.state.db.collection(name).findOne(id).exec()
//   $.subscribe(cb)           → api.query(name, {}) ohne die sofortige erste Lieferung (RxCollection.$ meldet nur Änderungen)
//   insert(doc) / upsert(doc) → api.put(name, doc) (lokal committet = aufgelöst); Grabstein `_deleted:true` → api.remove
//   doc.incrementalPatch(p)   → api.put(name, {...doc, ...p}) (flach wie RxDB, nicht mergePatch)
//   doc.remove()              → api.remove(name, id)
// Nur `writable` Collections nehmen Schreibvorgänge an (sonst Fehler code 'forbidden', wie eine fehlende Deklaration).
//
// Anhänge (CHATATT.md): business-chat.js legt einen Anhang als desktop_files-Zeile plus base64-Stücke in
// desktop_file_chunks ab. Beide Collections gehören in v4 nicht der Shell. `desktop_files`/`desktop_file_chunks` sind
// hier deshalb reine Schreibattrappen im Speicher: die Stücke werden gesammelt, die Dateizeile setzt sie zusammen,
// prüft sha256/Größe und legt die Bytes mit api.files.put in die eigene Dateiablage der Shell. Der BlobRef
// {ref, id, name, mime, size} landet in business_chats/<chat>.attachment_blobs; nach E30 darf ctox (Eigentümer von
// business_chats) die Datei darüber lesen. Kein Lesen oder Schreiben der echten desktop_files.
// v4-allow: debt L-42

// E47: gelesene Dokumente tragen _rev/_pending (Lese-Metadaten des Kerns); beim Zurückschreiben nie mitsenden.
const RX_META = ['_rev', '_pending', '_meta', '_attachments', '_deleted'];

function clean(doc) {
  const out = { ...(doc && typeof doc === 'object' ? doc : {}) };
  for (const key of RX_META) delete out[key];
  return out;
}

function forbidden(name, op) {
  return Object.assign(new Error(`business-chat: ${op} auf ${name} ist in Shell v4 nicht deklariert`), { code: 'forbidden' });
}

async function committed(handle) {
  await handle?.local;
  return handle;
}

export const BLOB_FIELD = 'attachment_blobs';
const CHAT_COLLECTION = 'business_chats';

function base64ToBytes(b64) {
  const bin = atob(String(b64 || ''));
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
  return out;
}

async function sha256Hex(bytes) {
  const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
  return [...digest].map((b) => b.toString(16).padStart(2, '0')).join('');
}

function attachmentError(message) {
  return Object.assign(new Error(`business-chat Anhang: ${message}`), { code: 'invalid' });
}

function unionBlobs(...lists) {
  const byRef = new Map();
  for (const list of lists) for (const b of Array.isArray(list) ? list : []) if (b?.ref && !byRef.has(`${b.ref}:${b.client_file_id || ''}`)) byRef.set(`${b.ref}:${b.client_file_id || ''}`, b);
  return [...byRef.values()];
}

export function createRawDb({ compat, collections, writable = [] }) {
  const api = compat?.api;
  const v2db = compat?.state?.db; // v4-allow: debt L-42
  if (!api || !v2db) return null;
  const canWrite = new Set(writable);
  // Anhang-Bühne: Stücke je file_id+generation, Blobs je Chat und je V2-file_id (für die Befehlsnutzlast, index.js).
  const chunkStage = new Map();
  const blobsByChat = new Map();
  const blobsByClientFile = new Map();
  const stagedRows = new Map();
  const toQuery = (q = {}) => {
    const out = {};
    if (q.selector) out.selector = q.selector;
    if (q.sort) out.sort = q.sort;
    if (Number.isSafeInteger(q.limit)) out.limit = q.limit;
    if (Number.isSafeInteger(q.skip) && q.skip > 0) out.skip = q.skip;
    return out;
  };

  function rawCollection(name) {
    const source = () => v2db.collection(name); // v4-allow: debt L-42
    const assertWritable = (op) => { if (!canWrite.has(name)) throw forbidden(name, op); };
    const put = async (doc) => {
      assertWritable('put');
      const row = clean(doc);
      if (!row.id) throw Object.assign(new Error(`business-chat: ${name} ohne id`), { code: 'invalid' });
      // business-chat.js kennt attachment_blobs nicht; jeder Schreibvorgang auf einen Chat behält die Liste.
      if (name === CHAT_COLLECTION && (blobsByChat.has(row.id) || row[BLOB_FIELD])) {
        row[BLOB_FIELD] = unionBlobs(row[BLOB_FIELD], blobsByChat.get(row.id));
      }
      return committed(api.put(name, row));
    };
    const remove = async (id) => {
      assertWritable('remove');
      return committed(api.remove(name, String(id)));
    };
    function wrap(row) {
      if (!row || typeof row !== 'object') return row;
      const data = { ...row };
      return Object.freeze({
        ...data,
        primary: data.id,
        toJSON: () => ({ ...data }),
        toMutableJSON: () => structuredClone(data),
        get: (field) => data[field],
        incrementalPatch: (patch) => put({ ...data, ...(patch && typeof patch === 'object' ? patch : {}) }),
        patch: (patch) => put({ ...data, ...(patch && typeof patch === 'object' ? patch : {}) }),
        remove: () => remove(data.id),
        incrementalRemove: () => remove(data.id),
      });
    }
    const liveSubscribe = (q, cb, { skipInitial = false } = {}) => {
      let first = skipInitial;
      const unsub = api.query(name, toQuery(q)).subscribe((rows) => {
        if (first) { first = false; return; }
        cb(Array.isArray(rows) ? rows.map(wrap) : []);
      });
      first = false;
      return { unsubscribe: () => { try { unsub?.(); } catch { /* still */ } } };
    };
    return {
      name,
      schema: { primaryPath: 'id' },
      find(q = {}) {
        return {
          async exec() { return (await source().find(q).exec()).map(wrap); },
          $: { subscribe: (cb) => liveSubscribe(q, typeof cb === 'function' ? cb : cb?.next?.bind(cb)) },
        };
      },
      findOne(id) {
        const key = id && typeof id === 'object' ? id?.selector?.id : id;
        return {
          async exec() { return wrap(await source().findOne(String(key ?? '')).exec()) ?? null; },
        };
      },
      async insert(doc) {
        if (doc?._deleted === true) { await remove(doc.id); return null; }
        await put(doc);
        return wrap(clean(doc));
      },
      async upsert(doc) {
        await put(doc);
        return wrap(clean(doc));
      },
      $: {
        // RxCollection.$: nur Änderungen, kein Anfangswert; die Rückrufe in business-chat.js sind Auslöser.
        subscribe: (cb) => liveSubscribe({}, () => cb?.({ collectionName: name }), { skipInitial: true }),
      },
    };
  }

  // BlobRef in den Chat: vorhandenes Dokument erweitern, sonst Mindestdokument (Pflichtfelder id/title/updated_at_ms);
  // business-chat.js schreibt beim nächsten persistChatState darüber (incrementalPatch behält das Feld).
  async function recordChatBlob(chatId, blob) {
    blobsByChat.set(chatId, unionBlobs(blobsByChat.get(chatId), [blob]));
    const chats = rawCollection(CHAT_COLLECTION);
    const existing = await chats.findOne(chatId).exec().catch(() => null);
    const base = existing ? existing.toJSON() : { id: chatId, title: 'Crew', owner_user_id: String(api.actor?.id || ''), messages: [], open: true, createdAt: Date.now() };
    await chats.upsert({ ...base, updated_at_ms: Date.now() });
  }

  async function storeFile(row) {
    const fileId = String(row.id || '');
    const generation = String(row.content_generation_id || '');
    const stage = chunkStage.get(`${fileId}\u0000${generation}`);
    if (!stage) throw attachmentError(`keine Stücke für ${row.name || fileId}`);
    const parts = [];
    for (let idx = 0; idx < stage.total; idx += 1) {
      if (typeof stage.parts[idx] !== 'string') throw attachmentError(`Stück ${idx + 1}/${stage.total} fehlt für ${row.name || fileId}`);
      parts.push(stage.parts[idx]);
    }
    const bytes = base64ToBytes(parts.join(''));
    const hash = await sha256Hex(bytes);
    if (row.content_hash && hash !== row.content_hash) throw attachmentError(`Prüfsumme passt nicht für ${row.name || fileId}`);
    if (Number.isSafeInteger(row.size_bytes) && row.size_bytes > 0 && row.size_bytes !== bytes.byteLength) throw attachmentError(`Größe passt nicht für ${row.name || fileId}`);
    const mime = String(row.mime_type || 'application/octet-stream');
    const put = await api.files.put(bytes, { name: String(row.name || 'attachment'), mime });
    if (put?.ref !== hash) throw attachmentError(`Dateiablage meldet andere Prüfsumme für ${row.name || fileId}`);
    chunkStage.delete(`${fileId}\u0000${generation}`);
    const blob = Object.freeze({ ref: put.ref, id: put.id, name: String(row.name || 'attachment'), mime, size: bytes.byteLength, client_file_id: fileId });
    blobsByClientFile.set(fileId, blob);
    if (row.linked_collection === CHAT_COLLECTION && row.linked_record_id) await recordChatBlob(String(row.linked_record_id), blob);
    return blob;
  }

  // Schreibattrappen für die Anhangablage von business-chat.js (nur die Formen von upsertRxDocument).
  function stagingCollection(name) {
    const accept = async (doc) => {
      const row = clean(doc);
      if (!row.id) throw attachmentError(`${name} ohne id`);
      if (name === 'desktop_file_chunks') {
        const key = `${row.file_id}\u0000${row.generation_id}`;
        const stage = chunkStage.get(key) || { total: Number(row.total) || 1, parts: [] };
        if (row.encoding !== 'base64' || !Number.isSafeInteger(row.idx) || row.idx < 0 || row.idx >= stage.total) throw attachmentError(`ungültiges Stück ${row.id}`);
        stage.parts[row.idx] = String(row.data || '');
        chunkStage.set(key, stage);
      } else if (row.kind === 'file') {
        await storeFile(row);
      }
      stagedRows.set(`${name}\u0000${row.id}`, row);
      return row;
    };
    const wrapStaged = (row) => row && Object.freeze({ ...row, toJSON: () => ({ ...row }), incrementalPatch: (patch) => accept({ ...row, ...(patch || {}) }) });
    return {
      name,
      schema: { primaryPath: 'id' },
      async upsert(doc) { return wrapStaged(await accept(doc)); },
      async insert(doc) { return wrapStaged(await accept(doc)); },
      findOne(id) { return { async exec() { return wrapStaged(stagedRows.get(`${name}\u0000${String(id ?? '')}`)) ?? null; } }; },
      find() { return { async exec() { return []; }, $: { subscribe: () => ({ unsubscribe() {} }) } }; },
      $: { subscribe: () => ({ unsubscribe() {} }) },
    };
  }

  const raw = {};
  for (const name of collections) raw[name] = rawCollection(name);
  if (canWrite.has(CHAT_COLLECTION) && api.files?.put) {
    raw.desktop_files = stagingCollection('desktop_files');
    raw.desktop_file_chunks = stagingCollection('desktop_file_chunks');
  }
  return Object.freeze({
    raw: Object.freeze(raw),
    /** v4-Blob zu einer V2-file_id (chatfile_…), sobald die Dateizeile gespeichert ist. */
    blobForClientFile: (fileId) => blobsByClientFile.get(String(fileId || '')) ?? null,
  });
}
