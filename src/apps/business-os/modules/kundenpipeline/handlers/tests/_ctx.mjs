// In-Memory-Attrappe des Handler-ctx (Shell v4, Handler-Vertrag fest).
// Kein Ersatz fuer die echte App-Laufzeit von "ctox sync v4": sie bildet nur
// read/get/put/patch/remove/transaction/command/model/http/cap/now/log nach,
// damit Handler lokal mit `node --test` laufen.
//
// Mango-Teilmenge: Gleichheit, $eq, $ne, $in, $nin, $exists, $gt, $gte, $lt,
// $lte, $and, $or. Fehlendes Feld != null (§16.10). `sort` und `limit`.
// Schema: optional die `collections.schema.json` der App; geprueft werden
// `required` und der Top-Level-`type` je Property (Teilmenge des Stubs).

import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { createAgentTaskSim } from '../../../../v4/tools/agent-sim/agent-task-sim.mjs';

// Handler importieren ctox:shared/prompt-template.js (§16.36 Nr. 3): Tests laufen mit der gemeinsamen Auflösung
// `node --import <business-os>/v4/tools/node/ctox-shared.mjs --test …` (kein eigener Lader je App).
export const APP_DIR = fileURLToPath(new URL('../../', import.meta.url));
// ctx.asset (§16.34) liest aus dem Paket der App, mit den Regeln der agent-task-Simulation (keine Tests, kein `..`).
export const packageAsset = createAgentTaskSim({ appId: 'kundenpipeline', appDir: APP_DIR }).asset;

const clone = (value) => (value === undefined ? undefined : JSON.parse(JSON.stringify(value)));

function typeOk(type, value) {
  if (!type) return true;
  const types = Array.isArray(type) ? type : [type];
  return types.some((t) => {
    if (t === 'null') return value === null;
    if (t === 'array') return Array.isArray(value);
    if (t === 'object') return value !== null && typeof value === 'object' && !Array.isArray(value);
    if (t === 'integer') return Number.isInteger(value);
    if (t === 'number') return typeof value === 'number' && Number.isFinite(value);
    return typeof value === t;
  });
}

function fieldOf(doc, path) {
  let cur = doc;
  for (const part of path.split('.')) {
    if (cur === null || typeof cur !== 'object' || !Object.prototype.hasOwnProperty.call(cur, part)) {
      return { present: false, value: undefined };
    }
    cur = cur[part];
  }
  return { present: true, value: cur };
}

function cmp(a, b) {
  if (typeof a === 'number' && typeof b === 'number') return a - b;
  if (typeof a === 'string' && typeof b === 'string') return a < b ? -1 : a > b ? 1 : 0;
  return NaN;
}

function matchOps(field, ops) {
  if (ops === null || typeof ops !== 'object' || Array.isArray(ops)) {
    return field.present && JSON.stringify(field.value) === JSON.stringify(ops);
  }
  const keys = Object.keys(ops);
  if (!keys.some((k) => k.startsWith('$'))) {
    return field.present && JSON.stringify(field.value) === JSON.stringify(ops);
  }
  return keys.every((op) => {
    const arg = ops[op];
    const v = field.value;
    switch (op) {
      case '$eq': return field.present && JSON.stringify(v) === JSON.stringify(arg);
      case '$ne': return !field.present || JSON.stringify(v) !== JSON.stringify(arg);
      case '$in': return field.present && arg.some((x) => JSON.stringify(x) === JSON.stringify(v));
      case '$nin': return !field.present || !arg.some((x) => JSON.stringify(x) === JSON.stringify(v));
      case '$exists': return field.present === Boolean(arg);
      case '$gt': return field.present && cmp(v, arg) > 0;
      case '$gte': return field.present && cmp(v, arg) >= 0;
      case '$lt': return field.present && cmp(v, arg) < 0;
      case '$lte': return field.present && cmp(v, arg) <= 0;
      default: throw { code: 'invalid', message: `unsupported mango operator ${op}` };
    }
  });
}

export function matches(doc, selector = {}) {
  return Object.entries(selector).every(([key, cond]) => {
    if (key === '$and') return cond.every((s) => matches(doc, s));
    if (key === '$or') return cond.some((s) => matches(doc, s));
    return matchOps(fieldOf(doc, key), cond);
  });
}

export async function loadSchema(url) {
  return JSON.parse(await readFile(url, 'utf8'));
}

export function createCtx({ seed = {}, now = 1_760_000_000_000, caps = {}, schema = null, commands = null } = {}) {
  const store = new Map(); // collection -> Map(id -> doc)
  const calls = [];
  const logs = [];
  const issued = [];
  let seq = 0;
  let clock = now;
  let writes = 0;

  const col = (name) => {
    if (!store.has(name)) store.set(name, new Map());
    return store.get(name);
  };
  for (const [name, docs] of Object.entries(seed)) {
    for (const doc of docs) col(name).set(doc.id, clone(doc));
  }

  function validate(collection, doc) {
    if (!schema) return;
    const def = schema.collections?.[collection];
    if (!def) {
      throw { code: 'forbidden', message: `collection ${collection} is not declared in collections.schema.json` };
    }
    for (const key of def.required || []) {
      if (!Object.prototype.hasOwnProperty.call(doc, key) || doc[key] === undefined) {
        throw { code: 'invalid', message: `${collection}.${key} is required`, path: key };
      }
    }
    for (const [key, prop] of Object.entries(def.properties || {})) {
      if (Object.prototype.hasOwnProperty.call(doc, key) && !typeOk(prop.type, doc[key])) {
        throw { code: 'invalid', message: `${collection}.${key} has wrong type`, path: key };
      }
    }
  }

  const accepted = () => ({ status: 'accepted', rev: `op:${++seq}`, seq });

  function makeApi(target) {
    return {
      async read(collection, query = {}) {
        const mango = query && Object.prototype.hasOwnProperty.call(query, 'selector') ? query : { selector: query };
        let docs = [...(target.get(collection)?.values() || [])].filter((d) => matches(d, mango.selector || {}));
        const sort = mango.sort || [{ id: 'asc' }];
        docs.sort((a, b) => {
          for (const entry of sort) {
            const [key, dir] = Object.entries(entry)[0];
            const r = cmp(fieldOf(a, key).value, fieldOf(b, key).value);
            if (r) return dir === 'desc' ? -r : r;
          }
          return cmp(a.id, b.id);
        });
        if (Number.isInteger(mango.limit)) docs = docs.slice(0, mango.limit);
        return clone(docs);
      },
      async get(collection, id) {
        return clone(target.get(collection)?.get(id) ?? null);
      },
      async put(collection, doc) {
        if (!doc || typeof doc.id !== 'string' || !doc.id) throw { code: 'invalid', message: 'id is required', path: 'id' };
        const stored = clone(doc);
        validate(collection, stored);
        if (!target.has(collection)) target.set(collection, new Map());
        target.get(collection).set(stored.id, stored);
        writes += 1;
        return accepted();
      },
      async patch(collection, id, fields) {
        const existing = target.get(collection)?.get(id);
        if (!existing) return { status: 'rejected', reason: 'not_found' };
        const next = { ...existing, ...clone(fields) };
        validate(collection, next);
        target.get(collection).set(id, next);
        writes += 1;
        return accepted();
      },
      async remove(collection, id) {
        target.get(collection)?.delete(id);
        writes += 1;
        return accepted();
      },
    };
  }

  const ctx = {
    ...makeApi(store),
    async transaction(fn) {
      const snapshot = new Map([...store].map(([k, v]) => [k, new Map([...v].map(([id, d]) => [id, clone(d)]))]));
      try {
        return await fn(makeApi(store));
      } catch (error) {
        store.clear();
        for (const [k, v] of snapshot) store.set(k, v);
        throw error;
      }
    },
    async command(type, payload) {
      const id = `cmd_test_${issued.length + 1}`;
      issued.push({ id, type, payload: clone(payload) });
      if (commands && commands[type]) return commands[type]({ id, type, payload });
      return { id };
    },
    async model(request) {
      if (caps.model) return caps.model(request);
      throw { code: 'unavailable', message: 'model not available in test ctx' };
    },
    async http() {
      throw { code: 'forbidden', message: 'http not declared' };
    },
    now: () => clock,
    asset: packageAsset,
    log(level, msg, data) {
      logs.push({ level, msg, data });
    },
  };

  // ctx.cap.<a>.<b>.<fn>(...) -> caps['a.b.fn'](...); fehlt der Eintrag: unavailable.
  const capProxy = (path) =>
    new Proxy(function () {}, {
      get(_t, prop) {
        if (prop === 'then') return undefined;
        return capProxy([...path, String(prop)]);
      },
      apply(_t, _this, args) {
        const name = path.join('.');
        calls.push({ name, args: clone(args) });
        const impl = caps[name];
        if (!impl) return Promise.reject({ code: 'unavailable', message: `capability ${name} not available` });
        return Promise.resolve().then(() => impl(...args));
      },
    });
  ctx.cap = capProxy([]);

  return {
    ctx,
    calls,
    logs,
    issued,
    setNow(ms) {
      clock = ms;
    },
    all(collection) {
      return clone([...(store.get(collection)?.values() || [])]);
    },
    doc(collection, id) {
      return clone(store.get(collection)?.get(id) ?? null);
    },
    writeCount: () => writes,
  };
}

export function cmd(type, payload = {}, extra = {}) {
  return {
    id: extra.id ?? `cmd_${type.replaceAll('.', '_')}`,
    type,
    payload,
    actor: extra.actor ?? { id: 'user_owner', role: 'chef' },
    app_id: extra.app_id ?? type.split('.')[0],
    app_ver: extra.app_ver ?? '0.0.0-test',
    ...(extra.record_id !== undefined ? { record_id: extra.record_id } : {}),
  };
}

export async function rejects(promise, code, pattern) {
  try {
    await promise;
  } catch (error) {
    if (error?.code !== code) throw new Error(`expected code ${code}, got ${error?.code}: ${error?.message}`);
    if (pattern && !pattern.test(String(error.message))) {
      throw new Error(`message ${JSON.stringify(error.message)} does not match ${pattern}`);
    }
    return error;
  }
  throw new Error(`expected rejection with code ${code}`);
}
