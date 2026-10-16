// In-memory ctx-Attrappe für threads-Handler (Shell v4, Strang H2).
// Bildet den Handler-Vertrag nach: read/get/put/patch/remove/transaction/
// command/model/http/cap/now/log/uuid. Mango-Teilmenge: implizites $eq,
// $eq $ne $gt $gte $lt $lte $in $nin $exists $and $or $not $elemMatch,
// Punktpfade, sort (asc/desc, Gleichstand nach id), skip, limit.
// Semantik nach §16.10: fehlendes Feld ≠ null; $exists:false trifft fehlende
// Felder; $eq:null trifft nur null.

import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createAgentTaskSim } from '../../../../v4/tools/agent-sim/agent-task-sim.mjs';

// Paketordner der App (für Skill-/Template-Auflösung der agent.task-Simulation, Kern §16.30/§16.34).
export const APP_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

const clone = (v) => (v === undefined ? undefined : JSON.parse(JSON.stringify(v)));

function getPath(doc, path) {
  let cur = doc;
  for (const part of path.split('.')) {
    if (cur === null || typeof cur !== 'object' || !(part in cur)) return { present: false };
    cur = cur[part];
  }
  return { present: cur !== undefined, value: cur };
}

function eq(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}

function cmp(a, b) {
  if (typeof a === 'number' && typeof b === 'number') return a - b;
  return String(a) < String(b) ? -1 : String(a) > String(b) ? 1 : 0;
}

function matchOp(field, op, arg) {
  const { present, value } = field;
  switch (op) {
    case '$eq':
      if (!present) return false;
      return Array.isArray(value) && !Array.isArray(arg) ? value.some((v) => eq(v, arg)) : eq(value, arg);
    case '$ne': return !matchOp(field, '$eq', arg);
    case '$gt': return present && value !== null && cmp(value, arg) > 0;
    case '$gte': return present && value !== null && cmp(value, arg) >= 0;
    case '$lt': return present && value !== null && cmp(value, arg) < 0;
    case '$lte': return present && value !== null && cmp(value, arg) <= 0;
    case '$in': return present && arg.some((a) => matchOp(field, '$eq', a));
    case '$nin': return !matchOp(field, '$in', arg);
    case '$exists': return arg ? present : !present;
    case '$not': return !matchCondition(field, arg);
    case '$elemMatch': return present && Array.isArray(value) && value.some((v) => matchSelector(v, arg));
    default: throw { code: 'invalid', message: `unsupported mango operator ${op}` };
  }
}

function matchCondition(field, cond) {
  if (cond !== null && typeof cond === 'object' && !Array.isArray(cond)
    && Object.keys(cond).length > 0 && Object.keys(cond).every((k) => k.startsWith('$'))) {
    return Object.entries(cond).every(([op, arg]) => matchOp(field, op, arg));
  }
  return matchOp(field, '$eq', cond);
}

export function matchSelector(doc, selector = {}) {
  return Object.entries(selector).every(([key, cond]) => {
    if (key === '$and') return cond.every((s) => matchSelector(doc, s));
    if (key === '$or') return cond.some((s) => matchSelector(doc, s));
    if (key === '$nor') return !cond.some((s) => matchSelector(doc, s));
    return matchCondition(getPath(doc, key), cond);
  });
}

export function runQuery(docs, query = {}) {
  const q = query && (query.selector || query.sort || query.limit !== undefined || query.skip !== undefined)
    ? query : { selector: query || {} };
  let out = docs.filter((d) => d._deleted !== true && matchSelector(d, q.selector || {}));
  const sort = (q.sort || []).map((s) => Object.entries(s)[0]);
  out.sort((a, b) => {
    for (const [k, dir] of sort) {
      const av = getPath(a, k).value;
      const bv = getPath(b, k).value;
      const c = av === bv ? 0 : av === undefined ? -1 : bv === undefined ? 1 : cmp(av, bv);
      if (c !== 0) return dir === 'desc' ? -c : c;
    }
    return String(a.id) < String(b.id) ? -1 : String(a.id) > String(b.id) ? 1 : 0;
  });
  const skip = q.skip || 0;
  out = out.slice(skip, q.limit === undefined ? undefined : skip + q.limit);
  return out.map(clone);
}

/**
 * makeCtx({seed, users, now, policy, commandResult, readCap})
 * - Workjet-Privatchat-Schutz und Supervisor-Bindung lesen foreign_collections aus seed
 *   (workjet_project_chats, workjet_worker_profile_bindings, workjet_projects, workjet_supervisor_bindings).
 * - seed: {collection: [doc,…]}
 * - users: {user_id: {display_name, active}}  → ctx.cap.directory.user
 * - policy: (permission, module) → {allowed, display_reason}
 * - commandResult: (type, payload, opts) → result of ctx.command
 * - readCap: max documents a single read returns (models the 200-cap)
 */
export function makeCtx(opts = {}) {
  const data = new Map();
  for (const [coll, docs] of Object.entries(opts.seed || {})) {
    data.set(coll, new Map(docs.map((d) => [d.id, clone(d)])));
  }
  let clock = opts.now ?? 1_700_000_000_000;
  let uuidCounter = 0;
  const calls = { commands: [], audit: [], policy: [], directory: [], logs: [], writes: [] };
  const readCap = opts.readCap ?? 200;

  const coll = (name) => {
    if (!data.has(name)) data.set(name, new Map());
    return data.get(name);
  };

  function makeStore(target) {
    const c = (name) => {
      if (!target.has(name)) target.set(name, new Map());
      return target.get(name);
    };
    return {
      async get(collection, id) {
        const doc = c(collection).get(id);
        return doc && doc._deleted !== true ? clone(doc) : null;
      },
      async read(collection, query) {
        const q = query || {};
        const limit = Math.min(q.limit ?? readCap, readCap);
        return runQuery([...c(collection).values()], { ...q, limit });
      },
      async put(collection, doc) {
        if (!doc || typeof doc.id !== 'string' || doc.id === '') throw { code: 'invalid', message: 'put requires doc.id' };
        const size = JSON.stringify(doc).length;
        if (size > 1024 * 1024) throw { code: 'quota', message: 'document too large' };
        c(collection).set(doc.id, clone(doc));
        calls.writes.push({ op: 'put', collection, id: doc.id });
        return { ok: true, id: doc.id };
      },
      async patch(collection, id, patch) {
        const cur = c(collection).get(id);
        if (!cur) throw { code: 'not_found', message: `${collection}/${id} not found` };
        c(collection).set(id, { ...cur, ...clone(patch), id });
        calls.writes.push({ op: 'patch', collection, id });
        return { ok: true, id };
      },
      async remove(collection, id) {
        c(collection).delete(id);
        calls.writes.push({ op: 'remove', collection, id });
        return { ok: true, id };
      },
    };
  }

  const root = makeStore(data);
  const users = opts.users || {};

  const ctx = {
    ...root,
    async transaction(fn) {
      const snapshot = new Map([...data].map(([k, v]) => [k, new Map([...v].map(([id, d]) => [id, clone(d)]))]));
      try {
        return await fn(makeStore(data));
      } catch (error) {
        data.clear();
        for (const [k, v] of snapshot) data.set(k, v);
        throw error;
      }
    },
    async command(type, payload, options = {}) {
      const entry = { type, payload: clone(payload), options: clone(options) };
      calls.commands.push(entry);
      if (opts.commandResult) return opts.commandResult(type, payload, options);
      const id = options.id || `cmd_auto_${calls.commands.length}`;
      return { ok: true, id, command_id: id, status: 'accepted' };
    },
    async model() { throw { code: 'unavailable', message: 'model not available in attrappe' }; },
    async http() { throw { code: 'forbidden', message: 'http not declared' }; },
    now: () => clock,
    uuid: () => `00000000-0000-4000-8000-${String(++uuidCounter).padStart(12, '0')}`,
    log: (level, msg, d) => calls.logs.push({ level, msg, data: d }),
    cap: {
      directory: {
        async user({ user_id }) {
          calls.directory.push(user_id);
          const u = users[user_id];
          // CAP-3b #844 Ergebnisform (Attrappe nach Vertrag): immer Objekt mit `found`.
          return u ? { found: true, user_id, display_name: u.display_name ?? user_id, active: u.active !== false } : { found: false, user_id, active: false };
        },
      },
      policy: {
        async module_decision({ permission, module }) {
          calls.policy.push({ permission, module });
          return opts.policy ? opts.policy(permission, module) : { allowed: true, display_reason: 'allowed' };
        },
      },
      audit: {
        async event(event) {
          calls.audit.push(clone(event));
          return { ok: true };
        },
      },
    },
  };

  // Strang SKL-A2: ctx.cap.agent.task + ctx.asset aus der Simulation; Rückrufe laufen gegen denselben Speicher.
  const sim = createAgentTaskSim({ appId: 'threads', appDir: APP_DIR, ctxFactory: () => ctx });
  sim.attach(ctx, null);

  return {
    ctx,
    calls,
    sim,
    setNow(v) { clock = v; },
    tick(ms = 1) { clock += ms; return clock; },
    doc(collection, id) { const d = coll(collection).get(id); return d ? clone(d) : null; },
    all(collection) { return [...coll(collection).values()].map(clone); },
  };
}

let seq = 0;
/** Befehl im v4-Vertragsformat {id, type, payload, actor, app_id, app_ver} (+ record_id/module wie die UI sendet). */
export function cmd(type, payload = {}, actor = { id: 'alice', role: 'user' }, extra = {}) {
  seq += 1;
  return { id: `cmd-test-${seq}`, type, payload, actor, app_id: 'threads', app_ver: '0.2.4', module: 'threads', ...extra };
}

export const USERS = {
  alice: { display_name: 'Alice' },
  bob: { display_name: 'Bob' },
  carol: { display_name: 'Carol' },
  admin: { display_name: 'Admin' },
  gone: { display_name: 'Gone', active: false },
};

export async function rejects(promise, code, messagePart) {
  try {
    await promise;
  } catch (error) {
    if (error?.code !== code) throw new Error(`expected code ${code}, got ${JSON.stringify(error)}`);
    if (messagePart && !String(error.message).includes(messagePart)) {
      throw new Error(`expected message containing "${messagePart}", got "${error.message}"`);
    }
    return error;
  }
  throw new Error(`expected rejection with ${code}`);
}
