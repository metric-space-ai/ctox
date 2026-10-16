// In-Memory-Attrappe des Handler-ctx (Shell v4, Strang H8) – nur für node --test.
// Übernommen aus customers/handlers/tests/_ctx.mjs (Strang H4) und verallgemeinert: App-ID, Fremd-Collections
// (foreign_collections) und Capability-Antworten (caps) sind je Test einstellbar. Vertrag §16.16:
// read/get/put/patch/remove/transaction/command/model/http/cap/now/log. Mango-Teilmenge nach §16.10.
// Schreibvorgänge auf eigene Collections werden gegen collections.schema.json geprüft (required, type, maxLength).

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const schemaPath = fileURLToPath(new URL('../../collections.schema.json', import.meta.url));
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
  return typeof value;
}

function validateDoc(collection, doc) {
  const schema = SCHEMA.collections[collection];
  if (!schema) return;
  for (const key of schema.required ?? []) {
    if (!(key in doc) || doc[key] === undefined) {
      throw ctxError('invalid', `${collection}.${key} is required`, { path: key });
    }
  }
  for (const [key, prop] of Object.entries(schema.properties ?? {})) {
    if (!(key in doc)) continue;
    const allowed = Array.isArray(prop.type) ? prop.type : [prop.type];
    const actual = typeOf(doc[key]);
    const ok = allowed.some((type) => type === actual || (type === 'integer' && Number.isInteger(doc[key])));
    if (!ok) throw ctxError('invalid', `${collection}.${key} must be ${allowed.join('|')}, got ${actual}`, { path: key });
    if (prop.maxLength && typeof doc[key] === 'string' && doc[key].length > prop.maxLength) {
      throw ctxError('invalid', `${collection}.${key} longer than ${prop.maxLength}`, { path: key });
    }
  }
}

// ---------------------------------------------------------------------------
// Mango

function getField(doc, path) {
  let cursor = doc;
  for (const key of path.split('.')) {
    if (cursor === null || typeof cursor !== 'object' || !Object.prototype.hasOwnProperty.call(cursor, key)) {
      return { present: false };
    }
    cursor = cursor[key];
  }
  return { present: true, value: cursor };
}

const sameType = (a, b) => (typeof a === 'number' && typeof b === 'number') || (typeof a === 'string' && typeof b === 'string');
const deepEqual = (a, b) => JSON.stringify(a) === JSON.stringify(b);

function matchCondition(field, condition) {
  const isOperatorObject = condition !== null && typeof condition === 'object' && !Array.isArray(condition)
    && Object.keys(condition).length > 0 && Object.keys(condition).every((key) => key.startsWith('$'));
  if (!isOperatorObject) return field.present && deepEqual(field.value, condition);
  return Object.entries(condition).every(([op, arg]) => {
    switch (op) {
      case '$eq': return field.present && deepEqual(field.value, arg);
      case '$ne': return !(field.present && deepEqual(field.value, arg));
      case '$gt': return field.present && sameType(field.value, arg) && field.value > arg;
      case '$gte': return field.present && sameType(field.value, arg) && field.value >= arg;
      case '$lt': return field.present && sameType(field.value, arg) && field.value < arg;
      case '$lte': return field.present && sameType(field.value, arg) && field.value <= arg;
      case '$in': return field.present && arg.some((item) => deepEqual(field.value, item));
      case '$nin': return !(field.present && arg.some((item) => deepEqual(field.value, item)));
      case '$exists': return field.present === Boolean(arg);
      case '$not': return !matchCondition(field, arg);
      case '$elemMatch':
        return field.present && Array.isArray(field.value) && field.value.some((item) => matchSelector(item, arg));
      default: throw ctxError('invalid', `unsupported mango operator ${op}`);
    }
  });
}

export function matchSelector(doc, selector = {}) {
  return Object.entries(selector).every(([key, condition]) => {
    if (key === '$and') return condition.every((sub) => matchSelector(doc, sub));
    if (key === '$or') return condition.some((sub) => matchSelector(doc, sub));
    if (key === '$nor') return !condition.some((sub) => matchSelector(doc, sub));
    if (key.startsWith('$')) throw ctxError('invalid', `unsupported mango operator ${key}`);
    return matchCondition(getField(doc, key), condition);
  });
}

const TYPE_RANK = (field) => {
  if (!field.present) return 0;
  const v = field.value;
  if (v === null) return 1;
  if (typeof v === 'number') return 2;
  if (typeof v === 'string') return 3;
  if (typeof v === 'boolean') return 4;
  return 5;
};

function compareBy(sort) {
  const keys = (sort ?? []).map((entry) => {
    const [field, dir] = Object.entries(entry)[0];
    return { field, dir: dir === 'desc' ? -1 : 1 };
  });
  keys.push({ field: 'id', dir: 1 });
  return (a, b) => {
    for (const { field, dir } of keys) {
      const fa = getField(a, field);
      const fb = getField(b, field);
      const ra = TYPE_RANK(fa);
      const rb = TYPE_RANK(fb);
      if (ra !== rb) return (ra - rb) * dir;
      if (ra >= 2 && fa.value !== fb.value) return (fa.value < fb.value ? -1 : 1) * dir;
    }
    return 0;
  };
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// ctx

export const APP_ID = 'esign';
const manifestPath = fileURLToPath(new URL(`../../../../v4/manifests/${APP_ID}.manifest.json`, import.meta.url));
export const MANIFEST = JSON.parse(readFileSync(manifestPath, 'utf8'));

// Eigene Collections = Manifest `collections`; Fremdzugriff = Manifest `foreign_collections` plus die in
// V4-HANDLERS.md beantragten Ergänzungen (`wishForeign`), damit Tests den künftigen Zugriff exakt abbilden.
export function createCtx({
  now = 1_700_000_000_000,
  seed = {},
  wishForeign = {},
  caps = {},
  appId = APP_ID,
} = {}) {
  const store = new Map(); // collection -> Map(id -> doc)
  const writes = [];
  const commands = [];
  const capCalls = [];
  const logs = [];
  let rev = 0;

  const own = new Set(Object.keys(MANIFEST.collections ?? {}));
  const foreign = { ...(MANIFEST.foreign_collections ?? {}), ...wishForeign };
  const access = (collection, op) => {
    if (own.has(collection)) return;
    const decl = foreign[collection];
    if (!decl || !decl.ops.includes(op)) {
      throw ctxError('forbidden', `${appId} may not ${op} ${collection}`);
    }
  };
  const table = (map, collection) => {
    if (!map.has(collection)) map.set(collection, new Map());
    return map.get(collection);
  };
  for (const [collection, docs] of Object.entries(seed)) {
    for (const doc of docs) table(store, collection).set(doc.id, clone(doc));
  }

  function makeDb(overlay) {
    const lookup = (collection, id) => {
      if (overlay?.has(collection) && overlay.get(collection).has(id)) return overlay.get(collection).get(id);
      return store.get(collection)?.get(id) ?? null;
    };
    const all = (collection) => {
      const ids = new Set([...(store.get(collection)?.keys() ?? []), ...(overlay?.get(collection)?.keys() ?? [])]);
      return [...ids].map((id) => lookup(collection, id)).filter(Boolean);
    };
    const commit = (collection, id, doc) => {
      const target = overlay ? table(overlay, collection) : table(store, collection);
      target.set(id, doc);
      if (!overlay) writes.push({ collection, id, op: doc ? 'put' : 'remove' });
    };
    const result = () => ({ status: 'accepted', rev: `r${++rev}` });
    const check = (collection, doc) => { if (own.has(collection)) validateDoc(collection, doc); };
    return {
      async read(collection, query = {}) {
        access(collection, 'read');
        const selector = query.selector ?? {};
        let rows = all(collection).filter((doc) => matchSelector(doc, selector)).sort(compareBy(query.sort));
        if (query.skip) rows = rows.slice(query.skip);
        if (query.limit !== undefined) rows = rows.slice(0, query.limit);
        return rows.map((doc) => deepFreeze(clone(doc)));
      },
      async get(collection, id) {
        access(collection, 'read');
        const doc = lookup(collection, id);
        return doc ? deepFreeze(clone(doc)) : null;
      },
      async put(collection, doc) {
        access(collection, 'write');
        if (!doc || typeof doc.id !== 'string' || doc.id === '') throw ctxError('invalid', 'doc.id is required');
        const stored = clone(doc);
        check(collection, stored);
        commit(collection, stored.id, stored);
        return result();
      },
      async patch(collection, id, partial) {
        access(collection, 'write');
        const current = lookup(collection, id);
        if (!current) throw ctxError('not_found', `${collection}/${id} not found`);
        const next = { ...clone(current), ...clone(partial), id };
        check(collection, next);
        commit(collection, id, next);
        return result();
      },
      async remove(collection, id) {
        access(collection, 'write');
        if (!lookup(collection, id)) throw ctxError('not_found', `${collection}/${id} not found`);
        commit(collection, id, null);
        return result();
      },
    };
  }

  const direct = makeDb(null);
  let inTransaction = false;
  const capProxy = (prefix) => new Proxy(function capFn() {}, {
    get(_target, name) {
      if (typeof name !== 'string' || name === 'then') return undefined;
      return capProxy(prefix ? `${prefix}.${name}` : name);
    },
    async apply(_target, _this, args) {
      if (inTransaction) throw ctxError('invalid', `cap.${prefix} called inside transaction (external effect)`);
      capCalls.push({ cap: prefix, args: clone(args) });
      const impl = caps[prefix];
      if (impl === undefined) return { ok: true, cap: prefix };
      return typeof impl === 'function' ? impl(...clone(args)) : clone(impl);
    },
  });

  const ctx = {
    ...direct,
    async transaction(fn) {
      const overlay = new Map();
      const tx = makeDb(overlay);
      inTransaction = true;
      let value;
      try {
        value = await fn(tx);
      } finally {
        inTransaction = false;
      }
      for (const [collection, docs] of overlay) {
        for (const [id, doc] of docs) {
          table(store, collection).set(id, doc);
          writes.push({ collection, id, op: doc ? 'put' : 'remove' });
        }
      }
      return value;
    },
    async command(type, payload) {
      const id = `cmd_child_${commands.length + 1}`;
      commands.push({ id, type, payload: clone(payload) });
      return { id };
    },
    async model() { throw ctxError('unavailable', 'model not available in test ctx'); },
    async http() { throw ctxError('forbidden', 'http not declared'); },
    cap: capProxy(''),
    now: () => now,
    log(level, msg, data) { logs.push({ level, msg, data: clone(data) }); },
  };

  // Test-Zugriffe (nicht Teil des Vertrags)
  ctx.$ = {
    doc: (collection, id) => clone(store.get(collection)?.get(id) ?? null),
    all: (collection) => [...(store.get(collection)?.values() ?? [])].filter(Boolean).map(clone),
    capsTo: (prefix) => capCalls.filter((call) => call.cap === prefix),
    writes,
    commands,
    capCalls,
    logs,
  };
  return ctx;
}

export function makeCommand(type, payload, { id = `cmd_${type}`, actor = { id: 'tester', role: 'admin' }, record_id } = {}) {
  const command = { id, type, payload, actor, app_id: APP_ID, app_ver: MANIFEST.app_ver ?? '0.0.0' };
  if (record_id !== undefined) command.record_id = record_id;
  return command;
}

export async function load(type) {
  return import(`../${type}.js`);
}

export async function run(ctx, type, payload, options) {
  const { default: handle } = await load(type);
  return handle(makeCommand(type, payload, options), ctx);
}
