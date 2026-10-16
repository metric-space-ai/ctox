// In-Memory-Attrappe des Handler-ctx (Shell v4, Handler-Vertrag fest).
// Nur für node --test. Kein Dateisystem, kein Netz.
//
// makeCtx({ collections, caps, now, actor, commands }) → { ctx, state }
//   collections: { name: [doc, …] }
//   caps:        verschachteltes Objekt mit Funktionen; jeder Aufruf wird in
//                state.capCalls als { path, args } protokolliert. Ein nicht
//                bereitgestellter Pfad wirft { code: 'unavailable' }.
//   now:         Zahl (ms) oder Funktion → ctx.now()

export function makeCtx(options = {}) {
  const state = {
    collections: new Map(),
    capCalls: [],
    commands: [],
    logs: [],
    revCounter: 0,
    files: new Map(),
  };
  for (const [name, docs] of Object.entries(options.collections || {})) {
    const map = new Map();
    for (const doc of docs) map.set(doc.id, clone(doc));
    state.collections.set(name, map);
  }
  const nowValue = options.now ?? 1_760_000_000_000;
  const coll = (name) => {
    if (!state.collections.has(name)) state.collections.set(name, new Map());
    return state.collections.get(name);
  };
  const nextRev = () => `${++state.revCounter}-test`;

  const store = {
    async read(collection, query = {}) {
      const docs = [...coll(collection).values()].filter((doc) =>
        matches(doc, query.selector || {}),
      );
      if (Array.isArray(query.sort)) {
        docs.sort((a, b) => {
          for (const entry of query.sort) {
            const [field, dir] = Object.entries(entry)[0];
            const cmp = compare(getPath(a, field), getPath(b, field));
            if (cmp !== 0) return dir === 'desc' ? -cmp : cmp;
          }
          return compare(a.id, b.id);
        });
      }
      const skip = query.skip || 0;
      const sliced = query.limit == null ? docs.slice(skip) : docs.slice(skip, skip + query.limit);
      return sliced.map(clone);
    },
    async get(collection, id) {
      const doc = coll(collection).get(id);
      return doc ? clone(doc) : null;
    },
    async put(collection, doc) {
      if (!doc || typeof doc.id !== 'string' || !doc.id) throw err('invalid', 'put requires doc.id');
      const rev = nextRev();
      coll(collection).set(doc.id, { ...clone(doc), _rev: rev });
      return { id: doc.id, rev };
    },
    async patch(collection, id, patch) {
      const current = coll(collection).get(id);
      if (!current) throw err('not_found', `${collection}/${id} not found`);
      const rev = nextRev();
      coll(collection).set(id, { ...current, ...clone(patch), id, _rev: rev });
      return { id, rev };
    },
    async remove(collection, id) {
      const existed = coll(collection).delete(id);
      return { id, rev: nextRev(), deleted: existed };
    },
  };

  const capProxy = (impl, path) =>
    new Proxy(function () {}, {
      get(_t, key) {
        if (typeof key === 'symbol') return undefined;
        if (key === 'then') return undefined;
        return capProxy(impl && typeof impl === 'object' ? impl[key] : undefined, [...path, key]);
      },
      apply(_t, _this, args) {
        state.capCalls.push({ path: path.join('.'), args: clone(args) });
        if (typeof impl !== 'function') {
          return Promise.reject(err('unavailable', `capability ${path.join('.')} not provided`));
        }
        return Promise.resolve().then(() => impl(...args));
      },
    });

  const ctx = {
    read: store.read,
    get: store.get,
    put: store.put,
    patch: store.patch,
    remove: store.remove,
    // Die Attrappe simuliert Atomarität per Schnappschuss/Rücksetzung.
    async transaction(fn) {
      const snapshot = new Map(
        [...state.collections].map(([name, map]) => [name, new Map([...map].map(([k, v]) => [k, clone(v)]))]),
      );
      try {
        return await fn(store);
      } catch (error) {
        state.collections = snapshot;
        throw error;
      }
    },
    async command(type, payload) {
      const id = `cmd-${state.commands.length + 1}`;
      state.commands.push({ id, type, payload: clone(payload) });
      return { id };
    },
    async model(request) {
      if (typeof options.model !== 'function') throw err('unavailable', 'model not provided');
      return options.model(request);
    },
    async http() {
      throw err('forbidden', 'http not declared');
    },
    cap: capProxy(options.caps || {}, []),
    // §16.32: ctx.files.put legt einen files-Datensatz an (ref = sha256 des Inhalts).
    files: {
      async put(data, meta = {}) {
        const { createHash } = await import('node:crypto');
        const bytes = typeof data === 'string' ? Buffer.from(data, 'utf8') : Buffer.from(data);
        const ref = createHash('sha256').update(bytes).digest('hex');
        state.files.set(ref, { bytes, meta: clone(meta) });
        const id = `file-${state.files.size}`;
        coll('files').set(id, { id, ref, name: meta.name, mime: meta.mime, size: bytes.length });
        return { id, ref };
      },
    },
    now() {
      return typeof nowValue === 'function' ? nowValue() : nowValue;
    },
    log(level, msg, data) {
      state.logs.push({ level, msg, data });
    },
  };
  return { ctx, state };
}

export function makeCommand(type, payload = {}, extra = {}) {
  return {
    id: extra.id || 'cmd-under-test',
    type,
    payload,
    actor: extra.actor || { id: 'user-chef', role: 'chef' },
    app_id: extra.app_id || 'test',
    app_ver: extra.app_ver || '0.0.0-test',
    ...(extra.record_id !== undefined ? { record_id: extra.record_id } : {}),
    ...(extra.client_context !== undefined ? { client_context: extra.client_context } : {}),
  };
}

export async function rejectsWithCode(promise, code) {
  try {
    await promise;
  } catch (error) {
    if (error && error.code === code) return error;
    throw new Error(`expected code ${code}, got ${JSON.stringify(error)} ${error?.stack || ''}`);
  }
  throw new Error(`expected rejection with code ${code}`);
}

function err(code, message) {
  return { code, message };
}

function clone(value) {
  return value === undefined ? undefined : JSON.parse(JSON.stringify(value));
}

function getPath(doc, path) {
  return path.split('.').reduce((v, k) => (v == null ? undefined : v[k]), doc);
}

function compare(a, b) {
  if (a === b) return 0;
  if (a === undefined) return -1;
  if (b === undefined) return 1;
  return a < b ? -1 : 1;
}

function matches(doc, selector) {
  for (const [key, cond] of Object.entries(selector)) {
    if (key === '$and') {
      if (!cond.every((s) => matches(doc, s))) return false;
      continue;
    }
    if (key === '$or') {
      if (!cond.some((s) => matches(doc, s))) return false;
      continue;
    }
    const value = getPath(doc, key);
    if (cond && typeof cond === 'object' && !Array.isArray(cond)) {
      for (const [op, arg] of Object.entries(cond)) {
        switch (op) {
          case '$eq': if (value !== arg) return false; break;
          case '$ne': if (value === arg) return false; break;
          case '$gt': if (!(value > arg)) return false; break;
          case '$gte': if (!(value >= arg)) return false; break;
          case '$lt': if (!(value < arg)) return false; break;
          case '$lte': if (!(value <= arg)) return false; break;
          case '$in': if (!arg.includes(value)) return false; break;
          case '$nin': if (arg.includes(value)) return false; break;
          case '$exists': if ((value !== undefined) !== Boolean(arg)) return false; break;
          default: throw err('invalid', `unsupported mango operator ${op}`);
        }
      }
    } else if (value !== cond) {
      return false;
    }
  }
  return true;
}
