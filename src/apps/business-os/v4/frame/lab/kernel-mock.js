// Kernel-Attrappe fuer das Rahmen-Labor: nur was der Rahmen braucht (Architektur §10.2a Nr. 8).
// apps = LiveQuery der sichtbaren Apps (AppEntry wie im Stub index.d.ts), activation.onChange,
// artifacts.manifest (layout: icon_asset/category), windowState, prefs (Pins), metrics/leader/identity als Platzhalter.

function liveDoc(value) {
  const subs = new Set();
  return {
    get: () => value,
    set(next) { value = next; for (const cb of subs) cb(value); },
    subscribe(cb) { subs.add(cb); cb(value); return () => subs.delete(cb); },
  };
}

// Befehls-Attrappe (nur mit commands: true): appApi mit command(); Statusdokumente stellt der Test über
// sim.cmd.deliver(id, state) zu (beliebige Folge, Wiederzustellung), das Schreibergebnis über sim.cmd.result(id, r).
function commandAppApi(appId, ver, cmds) {
  const unavailable = () => { throw Object.assign(new Error('nicht im Labor'), { name: 'CtoxError', code: 'unavailable' }); };
  const constDoc = (v) => ({ get: () => v, subscribe(cb) { cb(v); return () => {}; } });
  return {
    version: '4.0',
    app: { id: appId, ver },
    query: () => constDoc([]),
    get: () => constDoc(null),
    put: unavailable, patch: unavailable, remove: unavailable,
    command(type, payload, opts = {}) {
      const id = `cmd-${cmds.size + 1}`;
      let resolve;
      const result = new Promise((r) => { resolve = r; });
      const status = liveDoc({ status: 'queued' });
      cmds.set(id, { id, appId, type, payload, record_id: opts.record_id ?? null, status, resolve });
      return { id, status: { get: status.get, subscribe: status.subscribe }, local: Promise.resolve(), result, cancel: () => false };
    },
    setVisible() {},
    metrics: constDoc({ backlog: 0, outbox: { count: 0, oldest_ms: 0 }, link: { state: 'online', rtt_ms: 0, relay: false } }),
    actor: { id: '', display_name: '', role: '', permissions: constDoc([]) },
    files: { get: unavailable, put: unavailable },
  };
}

export const ACTIVE_SHELL = 'lab';
const encPath = (path) => String(path || '').replace(/^\.?\/+/, '').split('/').map(encodeURIComponent).join('/');
const rootOf = (appId, ver, sv) => `/~a/${encodeURIComponent(appId)}@${encodeURIComponent(String(ver))}~shell@${encodeURIComponent(sv)}/`;

export function createKernelMock({ apps = [], manifests = {}, commands = false } = {}) {
  const cmds = new Map();
  const binds = [];
  let rows = apps.map((a) => ({ ...a }));
  const appSubs = new Set();
  const activationSubs = new Set();
  const prefs = new Map();
  const windowState = new Map();
  const emitApps = () => { const snap = rows.map((r) => ({ ...r })); for (const cb of appSubs) cb(snap); };
  const kernel = {
    apps: {
      get: () => rows.map((r) => ({ ...r })),
      subscribe(cb) { appSubs.add(cb); cb(rows.map((r) => ({ ...r }))); return () => appSubs.delete(cb); },
    },
    activation: {
      get: (id) => liveDoc(rows.find((r) => r.id === id) || null),
      onChange(cb) { activationSubs.add(cb); return () => activationSubs.delete(cb); },
    },
    // Form wie kernel.artifacts.url/bind (§16.39, K-1d): url() nimmt die aktive Shell (hier fest ACTIVE_SHELL) und merkt
    // sich nichts; bind() liefert je Mount eine eingefrorene Bindung (sim.binds protokolliert die Aufrufe). Der
    // Labor-Server (serve.mjs) ignoriert die ~shell@-Bindung.
    artifacts: {
      async manifest(appId, ver) { return manifests[`${appId}@${ver}`] || manifests[appId] || null; },
      async read() { throw new Error('not in lab'); },
      url: (appId, ver, path) => rootOf(appId, ver, ACTIVE_SHELL) + encPath(path),
      bind(appId, ver) {
        const root = rootOf(appId, ver, ACTIVE_SHELL);
        binds.push(`${appId}@${ver}`);
        return Object.freeze({ appId, ver: String(ver), shellVer: ACTIVE_SHELL, libs: Object.freeze({}), root, url: (path) => root + encPath(path) });
      },
    },
    // §16.19 Nr. 3: Fensterzustand je Tab (hier: je Seite, im Speicher).
    windowState: {
      async get(key) { return windowState.has(key) ? structuredClone(windowState.get(key)) : null; },
      async put(key, value) { windowState.set(key, structuredClone(value)); },
      async delete(key) { windowState.delete(key); },
    },
    // Ersatz für shell_preferences (die Attrappe hat keine App-API): Schlüssel 'shell_preferences' → Zeile.
    prefs: {
      async get(key) { return prefs.has(key) ? prefs.get(key) : null; },
      set(key, value) { prefs.set(key, value); },
    },
    metrics: liveDoc({ backlog: 0, outbox: { count: 0, oldest_ms: 0 }, link: { state: 'online', rtt_ms: 6, relay: false } }),
    leader: liveDoc({ isLeader: true, leader_gen: 1, tab_id: 'lab' }),
    identity: { instance_id: 'lab', user_id: 'owner', locale: 'de' },
    ...(commands ? { appApi: (appId, ver) => commandAppApi(appId, ver, cmds) } : {}),
    // ---- Simulation ----
    sim: {
      subscriberCount: () => appSubs.size + activationSubs.size,
      binds,
      activeShell: () => ACTIVE_SHELL,
      addApp(entry, manifest) {
        if (manifest) manifests[`${entry.id}@${entry.active_ver}`] = manifest;
        rows = [...rows.filter((r) => r.id !== entry.id), { ...entry }];
        emitApps();
      },
      removeApp(id) { rows = rows.filter((r) => r.id !== id); emitApps(); },
      activate(id, ver, manifest) {
        if (manifest) manifests[`${id}@${ver}`] = manifest;
        rows = rows.map((r) => (r.id === id ? { ...r, active_ver: ver, activated_ver: ver, target_ver: null, state: 'activated' } : r));
        emitApps();
        for (const cb of activationSubs) cb({ app_id: id, ver, state: 'activated', activated_ver: ver, error: null });
      },
      prefs,
      cmd: {
        ids: () => [...cmds.keys()],
        deliver(id, state) { cmds.get(id).status.set({ ...state }); },
        result(id, r) { cmds.get(id).resolve(r); },
      },
    },
  };
  return kernel;
}
