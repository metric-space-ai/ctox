// Shell v4 – Kernel-Brücke. Der Rahmen (v4/frame/index.js) und der App-Host (app-host.js) sprechen nur mit dem
// Ergebnis von adaptKernel(). Bezugsstand: Kern-Stub 67229d2 (Architektur §16.19, CHANGES.md des Pins).
// Angenommen werden:
//   - der Kern-Stub (createStubKernel): Shell-Kernel-Objekte direkt am Kernel (apps, activation, status, windowState,
//     artifacts, metrics, leader, identity) plus appApi(),
//   - eine reine Shell-Kernel-API (gleiche Objekte ohne appApi, z. B. die Rahmen-Attrappe lab/kernel-mock.js),
//   - die Labor-Attrappe aus lab/app-host/api-attrappe.js (activation.current, Ereignisse mit appId, link up/down).
//
// Normalisierte Schnittstelle (alles, was Rahmen und Host benutzen):
//   tabId, appApi(appId, ver)
//   apps            LiveQuery<AppEntry> (activated_ver gesetzt oder null; layout immer mit den 9 Feldern aus §16.19 Nr. 6,
//                   styles immer ein Array)
//   activation      onChange(cb) → cb({app_id, ver, state, activated_ver, error, t0?, reverted?}),
//                   current(appId) → {ver, state}, reportFailed(appId, ver, reason) → Promise<boolean>
//   artifacts       manifest(appId, ver) → {entry, styles[], title, layout, …} | null, read,
//                   bind(appId, ver) → eingefrorene Bindung je MOUNT {shellVer, libs, root, url(path)} (§16.39, K-1d):
//                   die Shell ruft sie genau einmal je Mount; binding.url(path) ist synchron und baut
//                   /~a/<app>@<ver>~shell@<sv>[~lib:<name>@<major>.<minor>]*/<pfad> (Grammatik: Kopfkommentar v4/sw.js).
//                   url(appId, ver, path) → Artefakt-URL mit der AKTUELL aktiven Shell, merkt sich nichts (Icons,
//                   api.shell.assetUrl). Die Shell gibt nie eine Shell-Version mit. Beides baut allein der KERNEL; die
//                   Brücke reicht nur durch, fehlt eines, steht es in gaps (kein Ersatzweg, L-34 erledigt).
//   windowState     get/put/delete(key)   (§16.19 Nr. 3; je Tab und Nutzer macht der Kernel; ohne → gaps)
//   status          Zustandsstrom des Kernels (§16.19 Nr. 1) oder null
//   preferences(shellVer) → {actorId, get(), subscribe(cb), put(fields)} | null   (shell_preferences, §16.19 Nr. 3)
//   metrics         LiveDoc<Metrics> mit link.state ∈ online | offline (§16.19 Nr. 7)
//   leader, identity, gaps (fehlende Kernel-Teile), failedReports (Protokoll der reportFailed-Aufrufe)
//
// Kernel-Abonnements entstehen nur, solange ein Verbraucher abonniert ist: nach stopShell() hält ein Adapter
// keine Abos mehr (Messung „Abos nach 5 Shell-Tauschen konstant“).

const ADAPTED = Symbol.for('ctox.v4.adaptedKernel');
const cache = new WeakMap();
export const LAYOUT_KEYS = ['icon_asset', 'icon_svg', 'category', 'frame_palette', 'default_width', 'default_height', 'min_width', 'min_height', 'presentation'];
export const PREFERENCES_COLLECTION = 'shell_preferences';

export function normalizeLayout(layout) {
  const out = {};
  for (const k of LAYOUT_KEYS) out[k] = layout?.[k] ?? null;
  // Altbestand (module.json layout.group) als Kategorie lesen.
  if (out.category == null && layout?.group) out.category = String(layout.group);
  return out;
}

function liveConst(value) {
  return { get: () => value, subscribe(cb) { cb(value); return () => {}; } };
}

// Datenteil der App-API für Kernel ohne appApi (reine Shell-Kernel-Attrappe im Rahmen-Labor).
function minimalAppApi(appId, ver) {
  const unavailable = () => { throw Object.assign(new Error('App-API-Datenteil nicht verfügbar'), { name: 'CtoxError', code: 'unavailable' }); };
  return {
    version: '4.0',
    app: { id: appId, ver },
    query: () => liveConst([]),
    get: () => liveConst(null),
    put: unavailable, patch: unavailable, remove: unavailable, command: unavailable,
    setVisible() {},
    metrics: liveConst({ backlog: 0, outbox: { count: 0, oldest_ms: 0 }, link: { state: 'online', rtt_ms: 0, relay: false } }),
    actor: { id: '', display_name: '', role: '', permissions: liveConst([]) },
    files: { get: unavailable, put: unavailable },
  };
}

// §16.19 Nr. 7: nur online | offline. Die Labor-Attrappe meldet up/down.
function normLink(link) {
  if (!link) return link;
  const s = link.state;
  const state = s === 'online' || s === 'up' || s === 'connected' ? 'online' : 'offline';
  return state === s ? link : { ...link, state };
}
function normMetrics(m) {
  if (!m || !m.link) return m;
  const link = normLink(m.link);
  return link === m.link ? m : { ...m, link };
}

export function adaptKernel(k) {
  if (!k) throw new Error('adaptKernel: kernel fehlt');
  if (k[ADAPTED]) return k;
  if (cache.has(k)) return cache.get(k);
  const sk = k.apps?.subscribe ? k : (k.shell?.apps?.subscribe ? k.shell : (typeof k.shellApi === 'function' ? k.shellApi() : k));
  if (!sk?.apps?.subscribe) throw new Error('adaptKernel: kernel.apps (LiveQuery) fehlt');
  const gaps = new Set();
  const legacy = typeof sk.activation?.current === 'function'; // Labor-Attrappe: Ereignisse mit appId, Zeilen ohne activated_ver
  const identity = sk.identity || k.identity || { instance_id: 'local', user_id: 'local' };
  const tabId = k.tabId || sk.leader?.get?.()?.tab_id || `tab-${Math.random().toString(36).slice(2, 8)}`;
  const failedReports = [];

  // ---- Aktivierung -----------------------------------------------------------------------------------------
  const activatedVer = new Map(); // nur Attrappe: zuletzt aktivierte Version je App
  const normEvent = (ev) => ({
    app_id: String(ev.app_id ?? ev.appId ?? ''),
    ver: ev.ver == null ? null : String(ev.ver),
    state: ev.state,
    activated_ver: ev.activated_ver !== undefined ? ev.activated_ver : (ev.state === 'activated' ? String(ev.ver) : (activatedVer.get(ev.app_id ?? ev.appId) ?? null)),
    error: ev.error ?? null,
    t0: ev.t0,
    reverted: !!ev.reverted,
  });
  const reportFailedFn = sk.activation?.reportFailed;
  if (typeof reportFailedFn !== 'function') gaps.add('activation.reportFailed');
  const activation = {
    onChange(cb) {
      return sk.activation.onChange((raw) => {
        if (legacy && raw?.state === 'activated') activatedVer.set(raw.appId ?? raw.app_id, String(raw.ver));
        cb(normEvent(raw));
      });
    },
    // Laufende (aktivierte) Version: {ver, state}.
    current(appId) {
      if (legacy) {
        const c = sk.activation.current(appId);
        return { ver: c?.state === 'activated' ? String(c.ver) : (activatedVer.get(appId) ?? (c?.ver != null ? String(c.ver) : null)), state: c?.state ?? null };
      }
      const a = sk.activation.get?.(appId)?.get?.();
      const row = sk.apps.get?.()?.find((r) => r.id === appId);
      return { ver: a?.activated_ver ?? row?.activated_ver ?? null, state: a?.state ?? row?.state ?? null };
    },
    // §16.19 Nr. 2: Version für diesen Client failed, Kernel setzt den Zeiger auf die letzte funktionierende Version.
    async reportFailed(appId, ver, reason) {
      const rec = { appId, ver: String(ver), reason: String(reason ?? 'failed'), delivered: false, at: Date.now() };
      failedReports.push(rec);
      if (typeof reportFailedFn !== 'function') return false;
      try {
        const result = await reportFailedFn.call(sk.activation, appId, String(ver), rec.reason);
        rec.delivered = true;
        rec.activated_ver = result?.activated_ver ?? null;
        return true;
      } catch (e) {
        rec.error = String(e?.message ?? e);
        console.warn('reportFailed', e);
        return false;
      }
    },
  };

  // ---- Apps (Zeilen normalisiert) --------------------------------------------------------------------------
  const normRow = (row) => {
    if (!row) return row;
    let out = row;
    if (row.activated_ver === undefined) {
      const state = row.state ?? row.activation_state ?? 'activated';
      if (state === 'activated' && row.active_ver != null) activatedVer.set(row.id, String(row.active_ver));
      out = { ...row, state, activated_ver: activatedVer.get(row.id) ?? (row.active_ver != null ? String(row.active_ver) : null), target_ver: row.target_ver ?? null, error: row.error ?? null };
    }
    // §16.19 Nr. 6: layout + styles sind Teil jedes Eintrags (fehlende Felder = null).
    const hasLayout = !!out.layout && LAYOUT_KEYS.some((key) => out.layout[key] != null);
    return { ...out, layout: normalizeLayout(out.layout), styles: Array.isArray(out.styles) ? out.styles : [], layout_known: hasLayout };
  };
  const apps = {
    get: () => (sk.apps.get?.() || []).map(normRow),
    subscribe: (cb) => sk.apps.subscribe((rows) => cb((Array.isArray(rows) ? rows : []).map(normRow))),
  };

  // ---- Artefakte -------------------------------------------------------------------------------------------
  // §16.39 / K-1d: URL und Bindung je Mount baut der Kernel (Shell-Version und Lib-Minors); kein Ersatzweg.
  const missing = (name) => () => { throw Object.assign(new Error(`${name} fehlt im Kernel`), { name: 'CtoxError', code: 'unavailable' }); };
  const kernelUrl = typeof sk.artifacts?.url === 'function' ? sk.artifacts.url.bind(sk.artifacts) : null;
  const kernelBind = typeof sk.artifacts?.bind === 'function' ? sk.artifacts.bind.bind(sk.artifacts) : null;
  if (!kernelUrl) gaps.add('artifacts.url');
  if (!kernelBind) gaps.add('artifacts.bind');
  const url = kernelUrl || missing('artifacts.url');
  const bind = kernelBind || missing('artifacts.bind');
  const artifacts = {
    async manifest(appId, ver) {
      const m = await sk.artifacts?.manifest?.(appId, String(ver));
      if (!m) return null;
      const row = apps.get().find((x) => x.id === appId);
      // Ab 67229d2 trägt das Manifest styles (Reihenfolge = Ladereihenfolge). Nur ohne das Feld (Attrappe) gilt *.css.
      const styles = Array.isArray(m.styles) ? m.styles : Object.keys(m.files || {}).filter((p) => p.endsWith('.css'));
      const layout = normalizeLayout(m.layout || row?.layout);
      return { ...m, entry: m.entry || 'index.js', styles, title: m.title || row?.title || appId, layout };
    },
    url,
    bind,
    read: sk.artifacts?.read?.bind(sk.artifacts),
  };

  // ---- window_state (§16.19 Nr. 3) -------------------------------------------------------------------------
  // Der Kernel trennt nach Tab und Nutzer. Nur die Labor-Attrappe kennt keine Tabs: dort steht die Tab-ID im Schlüssel.
  const ws = sk.windowState || k.windowState || null;
  if (!ws || typeof ws.put !== 'function') gaps.add('windowState');
  const key = legacy ? (a) => `${tabId}/${a}` : (a) => a;
  const unavailableWs = () => Promise.reject(Object.assign(new Error('windowState fehlt im Kernel'), { name: 'CtoxError', code: 'unavailable' }));
  const windowState = ws && typeof ws.put === 'function'
    ? { put: (a, b) => ws.put(key(a), b), get: (a) => ws.get(key(a)), delete: (a) => (ws.delete || ws.del).call(ws, key(a)) }
    : { put: unavailableWs, get: unavailableWs, delete: unavailableWs };

  const appApiFn = typeof k.appApi === 'function' ? (a, v) => k.appApi(a, String(v)) : (typeof sk.appApi === 'function' ? (a, v) => sk.appApi(a, String(v)) : null);
  if (!appApiFn) gaps.add('appApi');

  // ---- shell_preferences (§16.19 Nr. 3): eine Zeile je Nutzer, id = actor.id, Zugriff über die App-API ----------
  function preferences(shellVer = 'dev') {
    if (appApiFn) {
      let api;
      try { api = appApiFn('shell', shellVer); } catch { api = null; }
      const actorId = api?.actor?.id;
      if (api && actorId && typeof api.put === 'function' && typeof api.get === 'function') {
        const live = () => api.get(PREFERENCES_COLLECTION, actorId);
        return {
          actorId,
          get: () => live().get() ?? null,
          subscribe: (cb) => live().subscribe((row) => cb(row ?? null)),
          async put(fields) {
            const prev = live().get() ?? {};
            // E47: _rev/_pending sind Lese-Metadaten, _deleted gehört dem Kern – nie zurückschreiben.
            const { _deleted, _rev, _pending, ...rest } = prev; // eslint-disable-line no-unused-vars
            const h = api.put(PREFERENCES_COLLECTION, { ...rest, ...fields, id: actorId });
            await h.local;
            return h;
          },
        };
      }
    }
    // Rahmen-Attrappe: Schlüssel-Wert-Ablage prefs.get/set (kein Live-Abo).
    const prefs = sk.prefs || k.prefs;
    if (prefs?.get && prefs?.set) {
      let row = null;
      return {
        actorId: identity.user_id || 'local',
        get: () => row,
        async load() { row = (await prefs.get(PREFERENCES_COLLECTION)) ?? null; return row; },
        subscribe: (cb) => { cb(row); return () => {}; },
        async put(fields) { row = { ...(row || {}), ...fields, id: identity.user_id || 'local' }; await prefs.set(PREFERENCES_COLLECTION, structuredClone(row)); },
      };
    }
    gaps.add('shell_preferences');
    return null;
  }

  // ---- Messwerte -------------------------------------------------------------------------------------------
  const rawMetrics = sk.metrics || k.metrics || null;
  const metrics = rawMetrics?.subscribe
    ? { get: () => normMetrics(rawMetrics.get?.()), subscribe: (cb) => rawMetrics.subscribe((m) => cb(normMetrics(m))) }
    : liveConst(null);

  const adapter = {
    [ADAPTED]: true,
    source: k,
    stub: k.sim ? k : null,
    tabId,
    appApi: appApiFn || minimalAppApi,
    apps,
    activation,
    artifacts,
    windowState,
    preferences,
    metrics,
    leader: sk.leader || k.leader || liveConst({ isLeader: true, leader_gen: 0, tab_id: '' }),
    identity,
    // §16.19 Nr. 1: Zustandsstrom des Kernels.
    status: sk.status?.subscribe ? sk.status : null,
    locale: identity.locale || k.locale || null,
    lab: k.lab || null,
    gaps,
    failedReports,
  };
  cache.set(k, adapter);
  return adapter;
}
