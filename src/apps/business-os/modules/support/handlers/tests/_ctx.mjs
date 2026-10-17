// In-Memory-Attrappe des Handler-ctx (Shell v4, Vertrag aus Strang H3).
// Bewusst klein: Mango-Teilmenge, Transaktion mit Rücknahme, Befehls-/Capability-Protokoll.
// Ersetzt NICHT den Handler-Stub von „ctox sync v4“ (§16.13); sobald der vorliegt, laufen die
// Tests gegen ihn (nur makeCtx austauschen).

import { pathToFileURL } from 'node:url';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const handlersDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');

// Handler dürfen ctox:shared/prompt-template.js importieren (§16.36 Nr. 3, Strang SKL-A1): Tests laufen mit
// `node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs` (gemeinsame Auflösung).

function clone(value) {
  return value === undefined ? undefined : JSON.parse(JSON.stringify(value));
}

function hasOwn(obj, key) {
  return obj !== null && typeof obj === 'object' && Object.prototype.hasOwnProperty.call(obj, key);
}

function getPath(doc, path) {
  let cur = doc;
  for (const part of path.split('.')) {
    if (!hasOwn(cur, part)) return { present: false, value: undefined };
    cur = cur[part];
  }
  return { present: true, value: cur };
}

function eq(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}

function cmp(a, b) {
  if (typeof a === 'number' && typeof b === 'number') return a - b;
  const sa = String(a);
  const sb = String(b);
  return sa < sb ? -1 : sa > sb ? 1 : 0;
}

// Mango-Semantik nach §16.10: fehlendes Feld ≠ null; $exists:false trifft fehlende Felder;
// $eq:null trifft nur null.
function matchCondition(field, cond, doc) {
  const { present, value } = getPath(doc, field);
  const isOps = cond !== null && typeof cond === 'object' && !Array.isArray(cond)
    && Object.keys(cond).length > 0 && Object.keys(cond).every((k) => k.startsWith('$'));
  if (!isOps) return present && eq(value, cond);
  return Object.entries(cond).every(([op, arg]) => {
    switch (op) {
      case '$eq': return present && eq(value, arg);
      case '$ne': return !(present && eq(value, arg));
      case '$gt': return present && value !== null && cmp(value, arg) > 0;
      case '$gte': return present && value !== null && cmp(value, arg) >= 0;
      case '$lt': return present && value !== null && cmp(value, arg) < 0;
      case '$lte': return present && value !== null && cmp(value, arg) <= 0;
      case '$in': return present && arg.some((item) => eq(value, item));
      case '$nin': return !(present && arg.some((item) => eq(value, item)));
      case '$exists': return arg ? present : !present;
      default: throw { code: 'invalid', message: `mango operator ${op} not supported by test ctx` };
    }
  });
}

export function matchSelector(selector, doc) {
  return Object.entries(selector || {}).every(([key, cond]) => {
    if (key === '$and') return cond.every((sub) => matchSelector(sub, doc));
    if (key === '$or') return cond.some((sub) => matchSelector(sub, doc));
    return matchCondition(key, cond, doc);
  });
}

function runQuery(docs, query) {
  const q = query && hasOwn(query, 'selector') ? query : { selector: query || {} };
  let rows = docs.filter((doc) => matchSelector(q.selector, doc));
  const sort = Array.isArray(q.sort) ? q.sort : [];
  rows.sort((a, b) => {
    for (const entry of sort) {
      const [field, dir] = typeof entry === 'string' ? [entry, 'asc'] : Object.entries(entry)[0];
      const c = cmp(getPath(a, field).value ?? 0, getPath(b, field).value ?? 0);
      if (c) return dir === 'desc' ? -c : c;
    }
    return cmp(a.id, b.id); // Gleichstände nach id (§16.10)
  });
  if (q.skip) rows = rows.slice(q.skip);
  if (q.limit !== undefined) rows = rows.slice(0, q.limit);
  return rows.map(clone);
}

export function makeCtx({ now = 1_760_000_000_000, collections = {}, cap = {}, tick = 0, commandResult } = {}) {
  const data = new Map();
  for (const [name, docs] of Object.entries(collections)) {
    data.set(name, new Map(docs.map((doc) => [doc.id, clone(doc)])));
  }
  let clock = now;
  const commands = [];
  const logs = [];
  const capCalls = [];
  let writes = 0;

  function coll(name) {
    if (!data.has(name)) data.set(name, new Map());
    return data.get(name);
  }

  function makeApi(target) {
    return {
      async read(collection, query) { return runQuery([...(target.get(collection)?.values() || [])], query); },
      async get(collection, id) { return clone(target.get(collection)?.get(id) ?? null); },
      async put(collection, doc) {
        if (!doc || typeof doc.id !== 'string' || !doc.id) throw { code: 'invalid', message: 'doc.id required' };
        if (!target.has(collection)) target.set(collection, new Map());
        target.get(collection).set(doc.id, clone(doc));
        writes += 1;
        return { id: doc.id, ok: true };
      },
      async patch(collection, id, partial) {
        const current = target.get(collection)?.get(id);
        if (!current) throw { code: 'not_found', message: `${collection}/${id}` };
        target.get(collection).set(id, { ...current, ...clone(partial) });
        writes += 1;
        return { id, ok: true };
      },
      async remove(collection, id) {
        target.get(collection)?.delete(id);
        writes += 1;
        return { id, ok: true };
      },
    };
  }

  function snapshot() {
    const copy = new Map();
    for (const [name, docs] of data) copy.set(name, new Map([...docs].map(([id, doc]) => [id, clone(doc)])));
    return copy;
  }

  // Protokoll der Capability-Aufrufe: cap.<pfad>(…) wird über Proxy aufgezeichnet.
  function wrapCap(obj, path) {
    if (typeof obj === 'function') {
      return async (...args) => {
        capCalls.push({ path, args: clone(args) });
        return obj(...args);
      };
    }
    if (obj && typeof obj === 'object') {
      return new Proxy(obj, {
        get(target, prop) {
          if (typeof prop !== 'string') return target[prop];
          return hasOwn(target, prop) ? wrapCap(target[prop], path ? `${path}.${prop}` : prop) : undefined;
        },
      });
    }
    return obj;
  }

  const ctx = {
    ...makeApi(data),
    async transaction(fn) {
      const backup = snapshot();
      try {
        return await fn(makeApi(data));
      } catch (error) {
        data.clear();
        for (const [name, docs] of backup) data.set(name, docs);
        throw error;
      }
    },
    async command(type, payload, opts) {
      const id = opts?.id ?? `cmd_test_${commands.length + 1}`;
      commands.push({ id, type, payload: clone(payload), opts: clone(opts) });
      return commandResult ? { id, ...commandResult } : { id };
    },
    async model() { throw { code: 'unavailable', message: 'model not available in test ctx' }; },
    async http() { throw { code: 'forbidden', message: 'http not declared' }; },
    cap: wrapCap(cap, ''),
    now() {
      const value = clock;
      clock += tick;
      return value;
    },
    log(level, msg, extra) { logs.push({ level, msg, data: extra }); },
  };

  return {
    ctx,
    docs(name) { return [...coll(name).values()].map(clone).sort((a, b) => cmp(a.id, b.id)); },
    doc(name, id) { return clone(coll(name).get(id) ?? null); },
    count(name) { return coll(name).size; },
    setNow(value) { clock = value; },
    commands,
    logs,
    capCalls,
    get writes() { return writes; },
  };
}

let commandCounter = 0;

export function command(type, payload = {}, { actor = { id: 'support-admin', role: 'admin' }, id, record_id } = {}) {
  commandCounter += 1;
  const cmd = { id: id || `cmd_${type.replace(/\./g, '_')}_${commandCounter}`, type, payload, actor, app_id: 'support', app_ver: 'test' };
  if (record_id !== undefined) cmd.record_id = record_id;
  return cmd;
}

const handlerCache = new Map();

export async function loadHandler(type) {
  if (!handlerCache.has(type)) {
    const mod = await import(pathToFileURL(resolve(handlersDir, `${type}.js`)).href);
    if (typeof mod.default !== 'function') throw new Error(`${type}.js has no default export`);
    handlerCache.set(type, mod.default);
  }
  return handlerCache.get(type);
}

export async function run(env, type, payload, options) {
  const handle = await loadHandler(type);
  const result = await handle(command(type, payload, options), env.ctx);
  const size = JSON.stringify(result).length;
  if (size > 1024 * 1024) throw new Error(`result of ${type} exceeds 1 MiB (${size})`);
  return result;
}

export async function runError(env, type, payload, options) {
  try {
    await run(env, type, payload, options);
  } catch (error) {
    return error;
  }
  throw new Error(`${type} was expected to throw`);
}
