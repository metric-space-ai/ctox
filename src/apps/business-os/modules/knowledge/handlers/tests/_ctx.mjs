// In-Memory-Attrappe des Handler-ctx (§16.16 + Trigger-Cursor §16.20) – nur für node --test.
// Muster: modules/customers/handlers/tests/_ctx.mjs (read/get/put/patch/remove/transaction/command/cap/now/log),
// ergänzt um ctx.cursor.get/set je Trigger und eine simulierte Fähigkeit ctx.cap.dataframe.window.
// Schreibvorgänge werden gegen das v4-Schema geprüft (required, type, maxLength): v4/schemas/knowledge.collections.
// schema.json ist die Schemaquelle (§16.21, Vorrang vor modules/knowledge/collections.schema.json, Strang KNOW2).
// files: {ref → Text|Uint8Array} für ctx.files.get (Kern §16.32); unbekannte ref → not_found.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const schemaPath = fileURLToPath(new URL('../../../../v4/schemas/knowledge.collections.schema.json', import.meta.url));
export const SCHEMA = JSON.parse(readFileSync(schemaPath, 'utf8'));

const ctxError = (code, message, extra = {}) => ({ code, message, ...extra });
const clone = (value) => (value === undefined ? undefined : JSON.parse(JSON.stringify(value)));

function deepFreeze(value) {
  if (value && typeof value === 'object') {
    Object.freeze(value);
    for (const key of Object.keys(value)) deepFreeze(value[key]);
  }
  return value;
}

function typeOf(value) {
  if (value === null) return 'null';
  if (Array.isArray(value)) return 'array';
  if (typeof value === 'number' && Number.isInteger(value)) return 'integer';
  return typeof value;
}

export function validateDoc(collection, doc) {
  const schema = SCHEMA.collections[collection];
  if (!schema) throw ctxError('invalid', `collection ${collection} is not declared in collections.schema.json`);
  for (const key of schema.required ?? []) {
    if (!(key in doc) || doc[key] === undefined) throw ctxError('invalid', `${collection}.${key} is required`, { path: key });
  }
  for (const [key, prop] of Object.entries(schema.properties ?? {})) {
    if (!(key in doc)) continue;
    const allowed = Array.isArray(prop.type) ? prop.type : [prop.type];
    const actual = typeOf(doc[key]);
    const ok = allowed.includes(actual) || (actual === 'integer' && allowed.includes('number'));
    if (!ok) throw ctxError('invalid', `${collection}.${key} must be ${allowed.join('|')}, got ${actual}`, { path: key });
    if (prop.maxLength && typeof doc[key] === 'string' && doc[key].length > prop.maxLength) {
      throw ctxError('invalid', `${collection}.${key} longer than ${prop.maxLength}`, { path: key });
    }
  }
}

function getField(doc, path) {
  let cursor = doc;
  for (const key of path.split('.')) {
    if (cursor === null || typeof cursor !== 'object' || !Object.prototype.hasOwnProperty.call(cursor, key)) return { present: false };
    cursor = cursor[key];
  }
  return { present: true, value: cursor };
}

const deepEqual = (a, b) => JSON.stringify(a) === JSON.stringify(b);

function matchCondition(field, condition) {
  const isOps = condition !== null && typeof condition === 'object' && !Array.isArray(condition)
    && Object.keys(condition).length > 0 && Object.keys(condition).every((key) => key.startsWith('$'));
  if (!isOps) return field.present && deepEqual(field.value, condition);
  return Object.entries(condition).every(([op, arg]) => {
    switch (op) {
      case '$eq': return field.present && deepEqual(field.value, arg);
      case '$ne': return !(field.present && deepEqual(field.value, arg));
      case '$in': return field.present && arg.some((item) => deepEqual(field.value, item));
      case '$exists': return field.present === Boolean(arg);
      default: throw ctxError('invalid', `unsupported mango operator ${op}`);
    }
  });
}

export function matchSelector(doc, selector = {}) {
  return Object.entries(selector).every(([key, condition]) => {
    if (key === '$and') return condition.every((sub) => matchSelector(doc, sub));
    if (key === '$or') return condition.some((sub) => matchSelector(doc, sub));
    if (key.startsWith('$')) throw ctxError('invalid', `unsupported mango operator ${key}`);
    return matchCondition(getField(doc, key), condition);
  });
}

function compareBy(sort) {
  const keys = (sort ?? []).map((entry) => {
    const [field, dir] = Object.entries(entry)[0];
    return { field, dir: dir === 'desc' ? -1 : 1 };
  });
  keys.push({ field: 'id', dir: 1 });
  return (a, b) => {
    for (const { field, dir } of keys) {
      const fa = getField(a, field).value;
      const fb = getField(b, field).value;
      if (fa === fb) continue;
      if (fa === undefined) return -dir;
      if (fb === undefined) return dir;
      return (fa < fb ? -1 : 1) * dir;
    }
    return 0;
  };
}

export function createCtx({ now = 1_700_000_000_000, seed = {}, dataframe = null, files = {} } = {}) {
  const store = new Map();
  const writes = [];
  const transactions = [];
  const logs = [];
  let cursorValue = null;
  let rev = 0;
  const table = (collection) => {
    if (!store.has(collection)) store.set(collection, new Map());
    return store.get(collection);
  };
  for (const [collection, docs] of Object.entries(seed)) for (const doc of docs) table(collection).set(doc.id, clone(doc));
  const own = (collection) => {
    if (!SCHEMA.collections[collection]) throw ctxError('forbidden', `knowledge may not access ${collection}`);
  };

  function makeDb(overlay, log) {
    const lookup = (collection, id) => {
      if (overlay?.has(collection) && overlay.get(collection).has(id)) return overlay.get(collection).get(id);
      return store.get(collection)?.get(id) ?? null;
    };
    const all = (collection) => {
      const ids = new Set([...(store.get(collection)?.keys() ?? []), ...(overlay?.get(collection)?.keys() ?? [])]);
      return [...ids].map((id) => lookup(collection, id)).filter(Boolean);
    };
    const commit = (collection, id, doc) => {
      if (overlay) {
        if (!overlay.has(collection)) overlay.set(collection, new Map());
        overlay.get(collection).set(id, doc);
      } else table(collection).set(id, doc);
      log.push({ collection, id, op: doc ? 'put' : 'remove' });
    };
    return {
      async read(collection, query = {}) {
        own(collection);
        let rows = all(collection).filter((doc) => matchSelector(doc, query.selector ?? {})).sort(compareBy(query.sort));
        if (query.skip) rows = rows.slice(query.skip);
        if (query.limit !== undefined) rows = rows.slice(0, query.limit);
        return rows.map((doc) => deepFreeze(clone(doc)));
      },
      async get(collection, id) {
        own(collection);
        const doc = lookup(collection, id);
        return doc ? deepFreeze(clone(doc)) : null;
      },
      async put(collection, doc) {
        own(collection);
        if (!doc || typeof doc.id !== 'string' || !doc.id) throw ctxError('invalid', 'doc.id is required');
        const stored = clone(doc);
        validateDoc(collection, stored);
        commit(collection, stored.id, stored);
        return { status: 'accepted', rev: `r${++rev}` };
      },
      async patch(collection, id, partial) {
        own(collection);
        const current = lookup(collection, id);
        if (!current) throw ctxError('not_found', `${collection}/${id} not found`);
        const next = { ...clone(current), ...clone(partial), id };
        validateDoc(collection, next);
        commit(collection, id, next);
        return { status: 'accepted', rev: `r${++rev}` };
      },
      async remove(collection, id) {
        own(collection);
        if (!lookup(collection, id)) throw ctxError('not_found', `${collection}/${id} not found`);
        commit(collection, id, null);
        return { status: 'accepted', rev: `r${++rev}` };
      },
    };
  }

  const direct = makeDb(null, writes);
  let inTransaction = false;
  const ctx = {
    ...direct,
    async transaction(fn) {
      const overlay = new Map();
      const log = [];
      inTransaction = true;
      try {
        const value = await fn(makeDb(overlay, log));
        for (const [collection, docs] of overlay) for (const [id, doc] of docs) {
          if (doc) table(collection).set(id, doc); else table(collection).delete(id);
        }
        writes.push(...log);
        transactions.push(log);
        return value;
      } finally {
        inTransaction = false;
      }
    },
    async command() { throw ctxError('forbidden', 'knowledge.view.window issues no follow-up commands'); },
    async model() { throw ctxError('unavailable', 'model not available in test ctx'); },
    async http() { throw ctxError('forbidden', 'http not declared'); },
    cap: {
      dataframe: {
        async window(request) {
          if (inTransaction) throw ctxError('invalid', 'capability call inside transaction');
          if (!dataframe) throw ctxError('forbidden', 'capability dataframe.window not declared');
          return dataframe.window(request);
        },
      },
    },
    files: {
      async get(ref) {
        if (inTransaction) throw ctxError('invalid', 'files.get inside transaction');
        if (!Object.prototype.hasOwnProperty.call(files, ref)) throw ctxError('not_found', `file ${ref} not found`);
        return files[ref];
      },
    },
    cursor: {
      async get() { return clone(cursorValue); },
      async set(value) { cursorValue = clone(value); },
    },
    now: () => now,
    log(level, msg, data) { logs.push({ level, msg, data: clone(data) }); },
  };
  ctx.$ = {
    all: (collection) => [...(store.get(collection)?.values() ?? [])].map(clone),
    doc: (collection, id) => clone(store.get(collection)?.get(id) ?? null),
    seed: (collection, doc) => table(collection).set(doc.id, clone(doc)),
    writes,
    transactions,
    logs,
    cursor: () => clone(cursorValue),
  };
  return ctx;
}

export function triggerCommand(doc, { op = 'update', seq, triggerId = 'knowledge.view.window:update' } = {}) {
  const change = { collection: 'knowledge_views', op, doc: clone(doc) };
  if (seq !== undefined) change.seq = seq;
  return { type: 'trigger', trigger_id: triggerId, change };
}
