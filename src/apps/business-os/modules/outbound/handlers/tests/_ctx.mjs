// In-memory ctx stand-in for the outbound v4 handler tests.
//
// Implements the fixed handler contract: read(collection, mango) / get / put /
// patch / remove / transaction / command / model / http / cap.<name>.<fn> /
// now / log. Mango subset: $eq $ne $gt $gte $lt $lte $in $nin $exists $and
// $or, dotted paths, implicit equality, plus `sort` and `limit`.
//
// Capabilities are recorded calls; a test supplies implementations through
// `caps: { 'mail.send': async (args) => ({...}) }`. A call without an
// implementation throws {code:'unavailable'} — exactly what a missing
// capability would do in the real runtime.

function clone(value) {
  return value === undefined ? undefined : JSON.parse(JSON.stringify(value));
}

function getPath(doc, path) {
  let cursor = doc;
  for (const key of path.split('.')) {
    if (cursor === null || typeof cursor !== 'object' || !(key in cursor)) return undefined;
    cursor = cursor[key];
  }
  return cursor;
}

function equal(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}

function compare(a, b) {
  if (typeof a !== typeof b) return null;
  if (typeof a === 'number' || typeof a === 'string') return a < b ? -1 : a > b ? 1 : 0;
  return null;
}

function matchOperator(value, op, arg) {
  switch (op) {
    case '$eq': return Array.isArray(value) && !Array.isArray(arg) ? value.some((v) => equal(v, arg)) : equal(value, arg);
    case '$ne': return !matchOperator(value, '$eq', arg);
    case '$gt': { const c = compare(value, arg); return c !== null && c > 0; }
    case '$gte': { const c = compare(value, arg); return c !== null && c >= 0; }
    case '$lt': { const c = compare(value, arg); return c !== null && c < 0; }
    case '$lte': { const c = compare(value, arg); return c !== null && c <= 0; }
    case '$in': return Array.isArray(arg) && arg.some((candidate) => matchOperator(value, '$eq', candidate));
    case '$nin': return Array.isArray(arg) && !arg.some((candidate) => matchOperator(value, '$eq', candidate));
    case '$exists': return arg ? value !== undefined : value === undefined;
    // Mango-Teilmenge (index.d.ts): $regex als JS-Regex, nur Flag `i` über $options (OUT3: sellify foreign read).
    case '$regex': {
      if (typeof value !== 'string') return false;
      const re = arg instanceof RegExp ? arg : new RegExp(arg, '');
      return re.test(value);
    }
    case '$options': return true;
    default: throw Object.assign(new Error(`unsupported mango operator ${op}`), { code: 'invalid' });
  }
}

export function matchesSelector(doc, selector) {
  if (!selector || typeof selector !== 'object') return true;
  for (const [key, condition] of Object.entries(selector)) {
    if (key === '$and') {
      if (!condition.every((sub) => matchesSelector(doc, sub))) return false;
      continue;
    }
    if (key === '$or') {
      if (!condition.some((sub) => matchesSelector(doc, sub))) return false;
      continue;
    }
    const value = getPath(doc, key);
    const isOperatorObject = condition !== null && typeof condition === 'object' && !Array.isArray(condition)
      && Object.keys(condition).length > 0 && Object.keys(condition).every((k) => k.startsWith('$'));
    if (isOperatorObject) {
      for (const [op, arg] of Object.entries(condition)) {
        const effective = op === '$regex' && condition.$options === 'i' && !(arg instanceof RegExp) ? new RegExp(arg, 'i') : arg;
        if (!matchOperator(value, op, effective)) return false;
      }
    } else if (!matchOperator(value, '$eq', condition)) {
      return false;
    }
  }
  return true;
}

function applySort(docs, sort) {
  if (!Array.isArray(sort) || sort.length === 0) return docs;
  return docs.sort((left, right) => {
    for (const entry of sort) {
      const [field, direction] = Object.entries(entry)[0];
      const c = compare(getPath(left, field), getPath(right, field));
      if (c) return direction === 'desc' ? -c : c;
    }
    return 0;
  });
}

function createStore(collections) {
  const api = {
    async read(collection, query = {}) {
      const mango = query && query.selector !== undefined ? query : { selector: query };
      const rows = Object.values(collections[collection] ?? {})
        .filter((doc) => !doc._deleted && matchesSelector(doc, mango.selector))
        .map(clone);
      const sorted = applySort(rows, mango.sort);
      const skipped = mango.skip ? sorted.slice(mango.skip) : sorted;
      return typeof mango.limit === 'number' ? skipped.slice(0, mango.limit) : skipped;
    },
    async get(collection, id) {
      const doc = collections[collection]?.[id];
      return doc && !doc._deleted ? clone(doc) : null;
    },
    async put(collection, doc) {
      if (!doc || typeof doc.id !== 'string' || !doc.id) {
        throw Object.assign(new Error('put requires doc.id'), { code: 'invalid' });
      }
      collections[collection] ??= {};
      collections[collection][doc.id] = clone(doc);
      return { ok: true, id: doc.id };
    },
    async patch(collection, id, patch) {
      const existing = collections[collection]?.[id];
      if (!existing || existing._deleted) throw Object.assign(new Error('not found'), { code: 'not_found' });
      collections[collection][id] = { ...existing, ...clone(patch), id };
      return { ok: true, id };
    },
    async remove(collection, id) {
      if (collections[collection]) delete collections[collection][id];
      return { ok: true, id };
    },
  };
  return api;
}

export function createTestCtx({ data = {}, now = 1_700_000_000_000, caps = {}, http = null, commandResult = null } = {}) {
  const state = { collections: clone(data) };
  const calls = { commands: [], caps: [], model: [], http: [], logs: [], writes: [], files: [] };
  let currentNow = now;

  const tracked = (store) => ({
    read: store.read,
    get: store.get,
    async put(collection, doc) { calls.writes.push({ op: 'put', collection, id: doc?.id }); return store.put(collection, doc); },
    async patch(collection, id, patch) { calls.writes.push({ op: 'patch', collection, id }); return store.patch(collection, id, patch); },
    async remove(collection, id) { calls.writes.push({ op: 'remove', collection, id }); return store.remove(collection, id); },
  });

  const makeCapProxy = (prefix) => new Proxy(() => {}, {
    get(_target, key) {
      if (typeof key !== 'string' || key === 'then') return undefined;
      return makeCapProxy(prefix ? `${prefix}.${key}` : key);
    },
    async apply(_target, _this, args) {
      calls.caps.push({ name: prefix, args: clone(args) });
      const impl = caps[prefix];
      if (!impl) throw Object.assign(new Error(`capability ${prefix} unavailable`), { code: 'unavailable' });
      return impl(...args);
    },
  });

  // Store revision: bumped by every committed write (outer write or transaction
  // commit). A transaction remembers the revision of its snapshot and is
  // rejected with {code:'conflict'} when another write committed in between —
  // the snapshot semantics of the real handler-kit ("concurrent modification
  // rejects transaction snapshot rather than losing another write"). Without
  // it, parallel transactions (Promise.all) silently overwrote each other.
  let revision = 0;
  const outer = tracked(createStore(state.collections));
  const bump = (fn) => async (...args) => { const result = await fn(...args); revision += 1; return result; };

  const ctx = {
    read: outer.read,
    get: outer.get,
    put: bump(outer.put),
    patch: bump(outer.patch),
    remove: bump(outer.remove),
    async transaction(fn) {
      const base = revision;
      const working = clone(state.collections);
      const tx = tracked(createStore(working));
      const result = await fn(tx); // on error the working copy is simply discarded
      if (revision !== base) {
        throw Object.assign(new Error('transaction snapshot conflict: concurrent modification'), { code: 'conflict' });
      }
      for (const key of Object.keys(state.collections)) delete state.collections[key];
      Object.assign(state.collections, working);
      revision += 1;
      return result;
    },
    // §16.21: ctx.command(type, payload, {id?, record_id?, module?, client_context?}) → {id, status, task_id?, reason?}
    async command(type, payload, options) {
      const id = options?.id ?? `cmd_test_${calls.commands.length + 1}`;
      calls.commands.push({ id, type, payload: clone(payload), ...(options !== undefined ? { options: clone(options) } : {}) });
      if (commandResult) return commandResult({ id, type, payload: clone(payload), options: clone(options) });
      return { id };
    },
    async model(request) {
      calls.model.push(clone(request));
      if (!caps.model) throw Object.assign(new Error('model unavailable'), { code: 'unavailable' });
      return caps.model(request);
    },
    async http(request) {
      calls.http.push(clone(request));
      if (!http) throw Object.assign(new Error('http not declared'), { code: 'forbidden' });
      return http(request);
    },
    cap: makeCapProxy(''),
    // §16.35 Nr. 1: ctx.files.put(bytes, {name, mime}) → {id, ref}; ref = sha256 des Inhalts (gleicher Inhalt = gleiche ref).
    files: {
      async put(bytes, meta) {
        const data = bytes instanceof Uint8Array ? bytes : new TextEncoder().encode(String(bytes));
        const hex = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', data)), (b) => b.toString(16).padStart(2, '0')).join('');
        calls.files.push({ ref: hex, size: data.length, meta: clone(meta), bytes: Array.from(data) });
        return { id: `file_${calls.files.length}`, ref: hex };
      },
    },
    now() { return currentNow; },
    log(level, msg, data) { calls.logs.push({ level, msg, data }); },
  };

  return {
    ctx,
    calls,
    state,
    setNow(value) { currentNow = value; },
    doc(collection, id) { return clone(state.collections[collection]?.[id] ?? null); },
    all(collection) { return Object.values(state.collections[collection] ?? {}).map(clone); },
  };
}

/** Build a contract command. */
export function cmd(type, payload = {}, extra = {}) {
  return {
    id: extra.id ?? `cmd_${type}`,
    type,
    payload,
    actor: extra.actor ?? { id: 'user_1', role: 'admin' },
    app_id: 'outbound',
    app_ver: '4.0.0-test',
    ...extra,
  };
}

/** Assert helper: run and capture a contract error. */
export async function rejects(promise) {
  try {
    await promise;
  } catch (error) {
    return error;
  }
  throw new Error('expected handler to throw');
}

/** Default caps used by tests that need audit events. */
export function auditCaps(extra = {}) {
  return { 'audit.event': async () => ({ ok: true }), ...extra };
}
