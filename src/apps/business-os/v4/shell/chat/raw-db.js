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
// v4-allow: debt L-42

const RX_META = ['_rev', '_meta', '_attachments', '_deleted'];

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

export function createRawDb({ compat, collections, writable = [] }) {
  const api = compat?.api;
  const v2db = compat?.state?.db; // v4-allow: debt L-42
  if (!api || !v2db) return null;
  const canWrite = new Set(writable);
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

  const raw = {};
  for (const name of collections) raw[name] = rawCollection(name);
  return Object.freeze({ raw: Object.freeze(raw) });
}
