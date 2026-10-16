// In-Memory-Attrappe des Handler-ctx (Shell v4, Handler-Vertrag fest).
// Nur für node --test. Kein Dateisystem, kein Netz.
//
// makeCtx({ collections, caps, now, actor, commands }) → { ctx, state }
//   collections: { name: [doc, …] }
//   caps:        verschachteltes Objekt mit Funktionen; jeder Aufruf wird in
//                state.capCalls als { path, args } protokolliert. Ein nicht
//                bereitgestellter Pfad wirft { code: 'unavailable' }.
//   now:         Zahl (ms) oder Funktion → ctx.now()
//
// Shell v4 §16.25: ctox_queue_tasks ist eine reine Projektion der Core-Queue
// (Wahrheit in ctox.sqlite3) und für Handler read-only. Jeder put/patch/remove
// darauf – direkt oder in ctx.transaction – wirft { code: 'forbidden' } und wird
// in state.forbiddenWrites protokolliert. Schreiben geht nur über
// ctx.cap.agent.queue.task.{update, delete} (Statuswechsel = task.update mit
// patch.route_status, Kernvertrag caps/queue.rs); die Attrappe stellt
// dafür eine Core-Simulation bereit (siehe coreQueueTaskCaps), die die Projektion
// als Stellvertreter der Kern-Zeile benutzt und business_commands mitführt.
// Ein in options.caps gesetzter gleichnamiger Pfad überschreibt die Simulation.
// state.writes protokolliert jeden Handler-Schreibversuch { op, collection, id }.

export function makeCtx(options = {}) {
  const state = {
    collections: new Map(),
    capCalls: [],
    commands: [],
    logs: [],
    writes: [],
    forbiddenWrites: [],
    tombstones: [],
    revCounter: 0,
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
  const guardWrite = (op, collection, id) => {
    state.writes.push({ op, collection, id: id ?? null });
    if (READ_ONLY_COLLECTIONS.has(collection)) {
      state.forbiddenWrites.push({ op, collection, id: id ?? null });
      throw err(
        'forbidden',
        `${collection} is a read-only projection of the core queue; use ctx.cap.agent.queue.task.{update,delete}`,
      );
    }
  };

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
      guardWrite('put', collection, doc?.id);
      if (!doc || typeof doc.id !== 'string' || !doc.id) throw err('invalid', 'put requires doc.id');
      const rev = nextRev();
      coll(collection).set(doc.id, { ...clone(doc), _rev: rev });
      return { id: doc.id, rev };
    },
    async patch(collection, id, patch) {
      guardWrite('patch', collection, id);
      const current = coll(collection).get(id);
      if (!current) throw err('not_found', `${collection}/${id} not found`);
      const rev = nextRev();
      coll(collection).set(id, { ...current, ...clone(patch), id, _rev: rev });
      return { id, rev };
    },
    async remove(collection, id) {
      guardWrite('remove', collection, id);
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
    cap: capProxy(
      mergeCaps(coreQueueTaskCaps(state, coll, () => (typeof nowValue === 'function' ? nowValue() : nowValue)), options.caps || {}),
      [],
    ),
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

export const READ_ONLY_COLLECTIONS = new Set(['ctox_queue_tasks']);

// Zielstatus, die ctx.cap.agent.queue.task.update als patch.route_status annimmt (V4-HANDLERS.md).
export const SETTABLE_ROUTE_STATUSES = [
  'pending',
  'blocked',
  'cancelled',
  'failed',
  'completed',
  'handled',
  'superseded',
];
const UPDATABLE_TASK_FIELDS = ['title', 'prompt', 'priority', 'status_note', 'route_status'];

// Core-Simulation der drei Schreib-Capabilities. Schreibt direkt in die Maps
// (am Wächter vorbei) – genau wie der Core, der die Projektion selbst erneuert.
function coreQueueTaskCaps(state, coll, now) {
  const requireTaskId = (args) => {
    const taskId = typeof args?.task_id === 'string' ? args.task_id.trim() : '';
    if (!taskId) throw err('invalid', 'task_id is required');
    return taskId;
  };
  const projection = () => coll('ctox_queue_tasks');
  const commands = () => coll('business_commands');
  const writeTask = (taskId, doc) => {
    const next = { ...doc, id: taskId, updated_at_ms: now(), _rev: `${++state.revCounter}-core` };
    projection().set(taskId, next);
    return clone(next);
  };
  return {
    agent: {
      queue: {
        task: {
          update(args) {
            const taskId = requireTaskId(args);
            const patch = args?.patch;
            if (patch === null || typeof patch !== 'object' || Array.isArray(patch)) {
              throw err('invalid', 'patch must be an object');
            }
            const fields = {};
            for (const [key, value] of Object.entries(patch)) {
              if (!UPDATABLE_TASK_FIELDS.includes(key)) throw err('invalid', `patch field ${key} is not updatable`);
              if (value === null || value === undefined) continue;
              if (typeof value !== 'string') throw err('invalid', `patch field ${key} must be text`);
              fields[key] = value;
            }
            if (!Object.keys(fields).length) throw err('invalid', 'nothing to update');
            if (fields.route_status !== undefined && !SETTABLE_ROUTE_STATUSES.includes(fields.route_status)) {
              throw err('invalid', `invalid status ${fields.route_status}`);
            }
            if (typeof fields.status_note === 'string') {
              if (fields.route_status !== undefined && !fields.status_note.trim()) delete fields.status_note;
              else if (fields.route_status !== undefined) fields.status_note = fields.status_note.trim();
            }
            const current = projection().get(taskId);
            if (!current) throw err('not_found', 'queue task not found');
            if (fields.route_status !== undefined) {
              if (current.route_status === 'leased' && fields.route_status !== 'cancelled') {
                throw err('conflict', 'leased task can only be cancelled');
              }
              // Statuswechsel spiegelt der Core in das business_commands-Dokument.
              const commandId = typeof current.command_id === 'string' ? current.command_id.trim() : '';
              if (commandId && commands().has(commandId)) {
                commands().set(commandId, {
                  ...commands().get(commandId),
                  status: fields.route_status,
                  observed_at_ms: now(),
                  _rev: `${++state.revCounter}-core`,
                });
              }
            }
            return writeTask(taskId, { ...current, ...fields });
          },
          delete(args) {
            const taskId = requireTaskId(args);
            if (args.command_id !== undefined && args.command_id !== null && typeof args.command_id !== 'string') {
              throw err('invalid', 'command_id must be text');
            }
            const current = projection().get(taskId);
            const explicit = typeof args.command_id === 'string' ? args.command_id.trim() : '';
            const fromProjection = typeof current?.command_id === 'string' ? current.command_id.trim() : '';
            const commandId = explicit || fromProjection || null;
            const deleted = projection().delete(taskId);
            state.tombstones.push({ collection: 'ctox_queue_tasks', id: taskId, at_ms: now() });
            if (commandId) {
              const command = commands().get(commandId);
              if (command) state.tombstones.push({ collection: 'business_commands', id: commandId, at_ms: now(), status: 'cancelled' });
              commands().delete(commandId);
            }
            return { task_id: taskId, command_id: commandId, deleted };
          },
        },
      },
    },
  };
}

// Blätter aus override gewinnen; Objekte werden rekursiv zusammengeführt.
function mergeCaps(base, override) {
  if (!isObject(base) || !isObject(override)) return override === undefined ? base : override;
  const out = { ...base };
  for (const [key, value] of Object.entries(override)) out[key] = mergeCaps(base[key], value);
  return out;
}

function isObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
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
