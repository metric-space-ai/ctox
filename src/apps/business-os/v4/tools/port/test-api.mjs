// Test-Helfer für App-Ports (Shell v4, Strang K).
//
// Verbindlich getestet wird gegen den Kern-Stub createStubKernel (~/.local/state/ctox-sync-v4/stub, nicht ändern),
// siehe modules/knowledge/tests/v4-api.test.mjs. Diese Datei liefert nur, was der Stub nicht abdeckt:
//   countingApi(api)            – zählt aktive query/get-Abos je Collection (Leck-Prüfung bei unmount/Remount)
//   withDialogAnswer(api, ans)  – feste Antwort für api.shell.dialog (Stub liefert spec.default ?? null)
// createTestApi(...) unten ist die ursprüngliche synchrone In-Memory-Attrappe nach app-api.d.ts (Rückfall, wenn der
// Stub nicht ladbar ist, z. B. reine Node-Unit-Tests ohne DOM). Bewusst klein; Semantik im Zweifel = Stub.

export class CtoxError extends Error {
  constructor(code, message, extra = {}) { super(message); this.code = code; Object.assign(this, extra); }
}

function clone(v) { return v === undefined ? v : structuredClone(v); }

function getPath(doc, path) {
  let cur = doc;
  for (const part of path.split('.')) {
    if (cur == null || typeof cur !== 'object' || !(part in cur)) return { found: false };
    cur = cur[part];
  }
  return { found: true, value: cur };
}

function cmp(a, b) {
  if (a === b) return 0;
  if (a === undefined) return -1;
  if (b === undefined) return 1;
  if (a === null) return -1;
  if (b === null) return 1;
  if (typeof a === typeof b) return a < b ? -1 : a > b ? 1 : 0;
  return String(a) < String(b) ? -1 : 1;
}

function matchOps(found, value, ops) {
  for (const [op, arg] of Object.entries(ops)) {
    switch (op) {
      case '$eq': if (!found || !deepEq(value, arg)) return false; break;
      case '$ne': if (found && deepEq(value, arg)) return false; break;
      case '$gt': if (!found || !(cmp(value, arg) > 0)) return false; break;
      case '$gte': if (!found || !(cmp(value, arg) >= 0)) return false; break;
      case '$lt': if (!found || !(cmp(value, arg) < 0)) return false; break;
      case '$lte': if (!found || !(cmp(value, arg) <= 0)) return false; break;
      case '$in': if (!found || !arg.some((x) => deepEq(value, x))) return false; break;
      case '$nin': if (found && arg.some((x) => deepEq(value, x))) return false; break;
      case '$exists': if (Boolean(found) !== Boolean(arg)) return false; break;
      case '$regex': {
        const re = arg instanceof RegExp ? arg : new RegExp(arg, ops.$options === 'i' ? 'i' : '');
        if (!found || typeof value !== 'string' || !re.test(value)) return false; break;
      }
      case '$options': break;
      case '$not': if (matchOps(found, value, arg)) return false; break;
      case '$elemMatch': if (!found || !Array.isArray(value) || !value.some((el) => matches(el, arg))) return false; break;
      default: throw new CtoxError('invalid', `Operator ${op} nicht erlaubt`, { path: op });
    }
  }
  return true;
}

function deepEq(a, b) { return JSON.stringify(a) === JSON.stringify(b); }

export function matches(doc, selector = {}) {
  for (const [key, cond] of Object.entries(selector || {})) {
    if (key === '$and') { if (!cond.every((s) => matches(doc, s))) return false; continue; }
    if (key === '$or') { if (!cond.some((s) => matches(doc, s))) return false; continue; }
    if (key === '$not') { if (matches(doc, cond)) return false; continue; }
    const { found, value } = getPath(doc, key);
    const isOps = cond && typeof cond === 'object' && !Array.isArray(cond) && Object.keys(cond).some((k) => k.startsWith('$'));
    if (isOps) { if (!matchOps(found, value, cond)) return false; }
    else if (!found || !deepEq(value, cond)) return false;
  }
  return true;
}

export function runQuery(docs, q = {}) {
  let out = docs.filter((d) => matches(d, q.selector));
  const sort = [...(q.sort || []), { id: 'asc' }];
  out.sort((a, b) => {
    for (const s of sort) {
      const [field, dir] = Object.entries(s)[0];
      const c = cmp(getPath(a, field).value, getPath(b, field).value);
      if (c) return dir === 'desc' ? -c : c;
    }
    return 0;
  });
  if (q.skip) out = out.slice(q.skip);
  if (q.limit != null) out = out.slice(0, q.limit);
  return out.map(clone);
}

function live(read, topic, bus) {
  return {
    get: () => read(),
    subscribe(cb) {
      const fn = () => cb(read());
      bus.on(topic, fn);
      cb(read());
      return () => bus.off(topic, fn);
    },
  };
}

function createBus() {
  const map = new Map();
  return {
    on(t, fn) { if (!map.has(t)) map.set(t, new Set()); map.get(t).add(fn); },
    off(t, fn) { map.get(t)?.delete(fn); },
    emit(t) { for (const fn of [...(map.get(t) || [])]) fn(); },
    count(t) { return map.get(t)?.size || 0; },
  };
}

let seq = 0;
function newId() { seq += 1; return `t${Date.now().toString(36)}${seq.toString(36)}`; }

export function createTestApi({ appId = 'app', appVer = '1.0.0', data = {}, locale = 'de', translations = {}, actor = {}, knownApps = ['ctox'], dialogAnswer = true } = {}) {
  const bus = createBus();
  const store = new Map();
  const zone = new Map();
  const commands = [];
  const navigations = [];
  const dialogs = [];
  const routeListeners = new Set();
  let currentLocale = locale;
  const coll = (name) => { if (!store.has(name)) store.set(name, new Map()); return store.get(name); };
  for (const [name, docs] of Object.entries(data)) for (const d of docs) coll(name).set(d.id, clone(d));
  const actorState = { id: 'u1', display_name: 'Test', role: 'admin', permissions: ['apps.source.edit'], ...actor };

  function write(op, collection, id, body) {
    if (typeof id !== 'string' || !id) {
      const err = new CtoxError('invalid', 'id fehlt', { path: 'id' });
      return { local: Promise.reject(err), result: Promise.resolve({ status: 'rejected', reason: 'invalid' }), cancel: () => false };
    }
    const c = coll(collection);
    if (op === 'put') c.set(id, clone(body));
    else if (op === 'patch') c.set(id, { ...(c.get(id) || { id }), ...clone(body) });
    else c.delete(id);
    bus.emit(`c:${collection}`);
    return { local: Promise.resolve(), result: Promise.resolve({ status: 'accepted', rev: `r${++seq}` }), cancel: () => false };
  }

  const shell = {
    setTitle(title) { shell._title = String(title); },
    registerHeader(spec) { shell._header = spec; },
    navigate(target, params = {}) {
      if (!knownApps.includes(target) && target !== appId) {
        zone.set('shell.navigate.unknown_target', { level: 'error', code: 'shell.navigate.unknown_target', text: `Navigationsziel unbekannt: ${target}` });
        return false;
      }
      navigations.push({ to: target, params: clone(params) });
      return true;
    },
    onRoute(cb) { routeListeners.add(cb); return () => routeListeners.delete(cb); },
    locale: live(() => currentLocale, 'locale', bus),
    t: (key) => translations[currentLocale]?.[key] ?? key,
    theme: live(() => ({}), 'theme', bus),
    dialog(spec) { dialogs.push(clone(spec)); return Promise.resolve(dialogAnswer); },
    menu(spec) { dialogs.push(clone(spec)); return Promise.resolve(null); },
    window: { size: live(() => ({ width: 1200, height: 720 }), 'win', bus), mode: live(() => 'normal', 'win', bus), close() {} },
    openFile() {},
  };

  const api = Object.freeze({
    version: '4.0',
    app: Object.freeze({ id: appId, ver: appVer }),
    query(collection, q = {}) {
      return live(() => runQuery([...coll(collection).values()], q), `c:${collection}`, bus);
    },
    get(collection, id) { return live(() => clone(coll(collection).get(id) ?? null), `c:${collection}`, bus); },
    put(collection, doc) { return write('put', collection, doc?.id, doc); },
    patch(collection, id, patch) { return write('patch', collection, id, patch); },
    remove(collection, id) { return write('remove', collection, id, null); },
    command(type, payload = {}) {
      if (typeof type !== 'string' || !type) throw new CtoxError('invalid', 'Befehlstyp fehlt', { path: 'type' });
      const id = newId();
      const doc = { id, type, payload: clone(payload), status: 'queued', app_id: appId, actor_id: actorState.id };
      commands.push(doc);
      const w = write('put', 'business_commands', id, doc);
      const status = live(() => {
        const d = coll('business_commands').get(id);
        const s = { status: d?.status || 'queued' };
        for (const k of ['progress', 'result', 'error']) if (d?.[k] !== undefined) s[k] = clone(d[k]);
        return s;
      }, 'c:business_commands', bus);
      return Object.freeze({ id, status, local: w.local, result: w.result, cancel: w.cancel });
    },
    setVisible() {},
    status: Object.freeze({
      report(item) { zone.set(item.code, { ...item }); },
      clear(code) { zone.delete(code); },
    }),
    metrics: live(() => ({ backlog: 0, outbox: { count: 0, oldest_ms: 0 }, link: { state: 'online', rtt_ms: 1, relay: false } }), 'metrics', bus),
    shell: Object.freeze(shell),
    actor: Object.freeze({ ...actorState, permissions: live(() => [...actorState.permissions], 'actor', bus) }),
    files: Object.freeze({
      async get() { throw new CtoxError('not_found', 'Attrappe ohne Dateien'); },
      put() { throw new CtoxError('unavailable', 'Attrappe ohne Dateien'); },
    }),
  });

  return {
    api,
    zone,
    commands,
    navigations,
    dialogs,
    shell,
    subscribers: (collection) => bus.count(`c:${collection}`),
    core: {
      upsert(collection, doc) { coll(collection).set(doc.id, clone(doc)); bus.emit(`c:${collection}`); },
      remove(collection, id) { coll(collection).delete(id); bus.emit(`c:${collection}`); },
      setCommandStatus(id, patch) {
        const c = coll('business_commands');
        c.set(id, { ...(c.get(id) || { id }), ...patch });
        bus.emit('c:business_commands');
      },
      docs(collection) { return [...coll(collection).values()].map(clone); },
      setLocale(next) { currentLocale = next; bus.emit('locale'); },
      route(params) { for (const cb of [...routeListeners]) cb({ from: 'shell', to: appId, params: clone(params) }); },
    },
  };
}

// ---- Ergänzungen zum Kern-Stub (createStubKernel), wo er nichts abdeckt ----

/** Umhüllt eine echte/Stub-API und zählt aktive query/get-Abos je Collection (Leck-Prüfung bei unmount/Remount). */
export function countingApi(api) {
  const active = new Map();
  const wrap = (collection, liveObj) => ({
    get: () => liveObj.get(),
    subscribe(cb) {
      active.set(collection, (active.get(collection) || 0) + 1);
      const off = liveObj.subscribe(cb);
      let done = false;
      return () => { if (done) return; done = true; active.set(collection, active.get(collection) - 1); off(); };
    },
  });
  const wrapped = delegate(api, {
    query: (collection, q) => wrap(collection, api.query(collection, q)),
    get: (collection, id) => wrap(collection, api.get(collection, id)),
  });
  return { api: wrapped, subscribers: (collection) => active.get(collection) || 0 };
}

/** Stub-Dialoge liefern `spec.default ?? null`; für Bestätigungspfade eine feste Antwort einsetzen. */
export function withDialogAnswer(api, answer) {
  const shell = delegate(api.shell, { dialog: (spec) => { api.shell.dialog(spec); return Promise.resolve(answer); } });
  return delegate(api, { shell });
}

// FR5-1: api.shell.dialog/menu über das Rahmen-Blatt (v4/frame/window-sheet.js) statt Stub-Antwort. Nachbau des
// Rahmens: `host` = Fensterelement, `root` = Inhaltsbereich (App-root), das Blatt hängt im Fenster, nicht im App-root.
//   const { api, sheet } = await withWindowSheet(api, windowEl, { root })
export async function withWindowSheet(api, host, { root = null } = {}) {
  const { createWindowSheet } = await import('../../frame/window-sheet.js');
  const sheet = createWindowSheet({ host, inertTargets: root ? () => [root] : null });
  const shell = delegate(api.shell, { dialog: (spec) => sheet.dialog(spec), menu: (spec) => sheet.menu(spec) });
  return { api: delegate(api, { shell }), sheet };
}

// Eingefrorene API-Objekte (Stub: Object.freeze) lassen sich nicht über Prototyp-Zuweisung ändern:
// neues Objekt, das alle Eigenschaften weiterreicht und einzelne ersetzt.
function delegate(target, overrides) {
  const out = {};
  for (const key of Object.keys(target)) {
    if (key in overrides) continue;
    Object.defineProperty(out, key, { enumerable: true, get: () => { const v = target[key]; return typeof v === 'function' ? v.bind(target) : v; } });
  }
  return Object.freeze(Object.assign(out, overrides));
}
