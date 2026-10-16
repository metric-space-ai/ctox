// Shell v4 – ehrlicher Verbindungszustand (Strang ZONE45, Kern §16.45, Owner-Mangel M-08).
//
// Grün darf nur erscheinen, wenn wirklich alles live ist: jede angeforderte Collection meldet „live“ (Verbindung oben,
// Backlog 0). Fehlt ein Beleg (keine Quelle, Collection nicht gemeldet, unbekannter V3-Zustand), ist das Ergebnis
// 'unknown' – nie 'ok'.
//
// Quellen, in dieser Reihenfolge:
//   V4  kernel.metrics.collections[name] = {state:'live'|'catching_up'|'offline'|'error', backlog, gen, since_ms, error?}
//       (Namen mit Generation `name@g<k>`; angefordert wird mit dem Grundnamen, es zählt die höchste Generation).
//       „Neu verbinden“ = kernel.reconnect() (voller Neuaufbau ohne Seiten-Reload).
//   V3  window.ctoxBusinessOsSyncDiagnostics (Funktion ODER Objekt), nur lesend, Rückfall bis zur Umstellung:
//       live         connectionStatus === 'connected' && queryReady === true
//       catching_up  connectionStatus === 'reconnecting' || reconnectingSince || lastLifecycleEvent?.code === 'peer_connect_timeout'
//                    (je Collection oder auf oberster Ebene; geht vor „live“)
//       error        connectionStatus ∈ error | failed | stopped | role-denied (über den Vertrag hinaus, siehe ZONE45.md)
//       sonst        unknown
//   nur metrics.link offline (Kernel ohne collections) → warn (source 'link'); online allein → 'unknown'.
//   keine Quelle → 'unknown'.
//
// Stufen: error (mind. eine Collection error) > warn (catching_up oder offline) > unknown (Beleg fehlt) > ok.
//
// Export:
//   createSyncState({ kernel, requested?, appCollections?, diagnostics?, pollMs? }) → {
//     getOverallState() → { level, pending: [names], errors: [{name, code, message}], unknown: [names], source }
//     getAppState(appId, collections?) → dasselbe für die Collections einer App (fehlt die Liste: appCollections(appId))
//     subscribe(fn) → off      fn(overall) sofort und bei jeder Änderung (ereignisgetrieben, V3 höchstens 1×/s abgefragt)
//     reconnect() → Promise<boolean>   kernel.reconnect(); ohne V4-Kernel false (V3 ist nur lesend)
//     canReconnect → boolean, setRequested(names), refresh(), destroy()
//   }
//   currentSyncState() → die zuletzt erzeugte, nicht zerstörte Instanz (für die Kopfleiste/TOPBAR).
//   requested: Collections, die die Shell angefordert hat (Array oder Funktion). Leer = alle gemeldeten Collections.

export const SYNC_CODE = 'shell.sync';
// v4-allow: zone (Zonen-Texte des Badge-Eintrags und des Zustandspunkts, §16.45)
export const SYNC_TEXT = Object.freeze({ warn: 'Verbindung wird wiederhergestellt', error: 'Verbindung unterbrochen', action: 'Neu verbinden' });
export const MIN_POLL_MS = 1000;
const V3_EVENT = 'ctox-business-os-sync-diagnostics';
const V3_ERROR = new Set(['error', 'failed', 'stopped', 'role-denied']);
const RANK = { error: 3, warn: 2, unknown: 1, ok: 0 };

let latest = null;
export function currentSyncState() { return latest; }

const baseName = (name) => String(name).replace(/@g\d+$/, '');
const listOf = (v) => {
  let x = v;
  try { if (typeof x === 'function') x = x(); } catch { x = null; }
  return Array.isArray(x) ? [...new Set(x.filter((n) => typeof n === 'string' && n))] : null;
};

function readV3(src) {
  try {
    // src ist die Diagnose selbst oder ein Zugriff darauf; die Diagnose kann ihrerseits Funktion ODER Objekt sein.
    let d = typeof src === 'function' ? src() : src;
    if (typeof d === 'function') d = d();
    return d && typeof d === 'object' && d.collections && typeof d.collections === 'object' ? d : null;
  } catch { return null; }
}

// V3-Eintrag einer Collection → Zustand im V4-Vokabular (state, error?).
export function v3CollectionState(c, top = null) {
  const yellow = (x) => !!x && (x.connectionStatus === 'reconnecting' || !!x.reconnectingSince || x.lastLifecycleEvent?.code === 'peer_connect_timeout');
  if (!c || typeof c !== 'object') return yellow(top) ? { state: 'catching_up' } : null;
  if (yellow(c) || yellow(top)) return { state: 'catching_up' };
  if (V3_ERROR.has(c.connectionStatus)) return { state: 'error', error: { code: String(c.connectionStatus), message: String(c.lastError?.message ?? c.connectionStatus) } };
  if (c.connectionStatus === 'connected' && c.queryReady === true) return { state: 'live' };
  return { state: 'unknown' };
}

// Ein Messwert-Objekt (V4) bzw. eine Diagnose (V3) → Zustand je Name.
function readSource(metrics, diag) {
  const m = metrics && typeof metrics === 'object' ? metrics : null;
  if (m && m.collections && typeof m.collections === 'object') {
    const byBase = new Map();
    for (const [key, v] of Object.entries(m.collections)) {
      if (!v || typeof v !== 'object') continue;
      const b = baseName(key);
      const prev = byBase.get(b);
      if (!prev || (Number(v.gen) || 1) >= (Number(prev.gen) || 1)) byBase.set(b, v);
    }
    const offline = m.link?.state && m.link.state !== 'online';
    return {
      source: 'v4',
      names: () => [...byBase.keys()],
      // Nicht gemeldet: ohne Verbindung offline, sonst fehlt der Beleg (unknown) – nie live.
      get: (name) => byBase.get(baseName(name)) || { state: offline ? 'offline' : 'unknown' },
      empty: () => (offline ? 'warn' : 'ok'),
      // Rückstand ohne Collection-Zuordnung (metrics.backlog größer als die Summe der Collection-Rückstände, z. B.
      // sim.setBacklog(n) ohne Namen): gehört zu keiner App, macht den GESAMTzustand aber nicht grün.
      unassigned: Math.max(0, (Number(m.backlog) || 0) - [...byBase.values()].reduce((a, v) => a + (Number(v.backlog) || 0), 0)),
    };
  }
  const d = readV3(diag);
  if (d) {
    return {
      source: 'v3',
      names: () => Object.keys(d.collections),
      get: (name) => v3CollectionState(d.collections[name], d) || { state: 'unknown' },
      empty: () => (v3CollectionState(null, d) ? 'warn' : 'unknown'),
    };
  }
  // Nur metrics.link (Kernel ohne §16.45, z. B. Pin 68b83a3, Labor-Attrappen): offline ist ein Beleg für warn,
  // online ist KEIN Beleg für live (bleibt unknown). Einzige Quelle für „Verbindung weg“ (früher metrics.link der Zone).
  if (m?.link?.state === 'offline') {
    return { source: 'link', names: () => [], get: () => ({ state: 'offline' }), empty: () => 'warn' };
  }
  return { source: 'none', names: () => [], get: () => ({ state: 'unknown' }), empty: () => 'unknown' };
}

function evaluate(src, names) {
  const pending = [], errors = [], unknown = [], collections = {};
  for (const name of names) {
    const c = src.get(name);
    collections[name] = c.state;
    if (c.state === 'error') errors.push({ name, code: String(c.error?.code ?? 'error'), message: String(c.error?.message ?? c.error?.code ?? 'error') });
    else if (c.state === 'catching_up' || c.state === 'offline') pending.push(name);
    else if (c.state !== 'live') unknown.push(name);
  }
  let level;
  if (src.source === 'none') level = 'unknown';
  else if (errors.length) level = 'error';
  else if (pending.length) level = 'warn';
  else if (unknown.length) level = 'unknown';
  else if (!names.length) level = src.empty();
  else level = 'ok';
  return { level, pending, errors, unknown, source: src.source, collections };
}

export function createSyncState({ kernel = null, requested = null, appCollections = null, diagnostics = undefined, pollMs = MIN_POLL_MS } = {}) {
  const win = typeof window !== 'undefined' ? window : null;
  // v4-allow: debt L-41 (V3-Rückfall, Löschung Sa 17.10.2026 mit V4-Core auf welsch)
  const diagSource = diagnostics !== undefined ? diagnostics : () => win?.ctoxBusinessOsSyncDiagnostics;
  const metrics = kernel?.metrics && typeof kernel.metrics.subscribe === 'function' ? kernel.metrics : null;
  const raw = kernel?.source && kernel.source !== kernel ? kernel.source : null; // adaptKernel: Original-Kernel
  const owner = [kernel, raw, kernel?.shell, raw?.shell].find((o) => o && typeof o.reconnect === 'function') || null;
  const listeners = new Set();
  let shellRequested = requested;
  let lastMetrics = null;
  let lastSig = '';
  let pollTimer = null;
  let destroyed = false;
  let offMetrics = null;

  const src = () => readSource(metrics ? (lastMetrics ?? safeGet()) : null, diagSource);
  function safeGet() { try { return metrics?.get?.() ?? null; } catch { return null; } }
  const namesFor = (s, list) => (list && list.length ? list : s.names());

  function getOverallState() {
    const s = src();
    const r = evaluate(s, namesFor(s, listOf(shellRequested)));
    if (s.unassigned > 0 && (r.level === 'ok' || r.level === 'unknown')) return { ...r, level: 'warn', backlog: s.unassigned };
    return r;
  }
  function getAppState(appId, collections) {
    const s = src();
    const list = listOf(collections !== undefined && collections !== null ? collections : (appCollections ? () => appCollections(appId) : null));
    // Ohne angeforderte Collections zählt nur die Verbindung (V4) bzw. der oberste V3-Zustand.
    return evaluate(s, list || []);
  }

  function notify(force = false) {
    if (destroyed) return;
    const overall = getOverallState();
    const sig = JSON.stringify(overall);
    if (!force && sig === lastSig) {
      // Gesamtzustand gleich, App-Zustände können sich trotzdem geändert haben (andere Collections): Abonnenten
      // mit { app: true } bekommen jede Quellenänderung.
      for (const l of listeners) if (l.app) { try { l.fn(overall); } catch (e) { console.warn('[sync-state]', e); } }
      return;
    }
    lastSig = sig;
    for (const l of listeners) { try { l.fn(overall); } catch (e) { console.warn('[sync-state]', e); } }
  }

  // Ereignisse: V4 über das Messwert-Abo, V3 über das Diagnose-Ereignis; Abfrage (≥ 1 s) nur ohne V4-Collections,
  // weil V3 sein Diagnose-Objekt auch ohne Ereignis an Ort und Stelle ändert.
  const onV3 = () => notify();
  function hasV4() { const m = lastMetrics ?? safeGet(); return !!(m && m.collections && typeof m.collections === 'object'); }
  function syncPolling() {
    const want = !destroyed && listeners.size > 0 && !hasV4();
    if (want && !pollTimer) { pollTimer = setInterval(() => notify(), Math.max(MIN_POLL_MS, Number(pollMs) || MIN_POLL_MS)); pollTimer.unref?.(); }
    if (!want && pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  }
  function start() {
    if (metrics && !offMetrics) {
      offMetrics = metrics.subscribe((m) => { lastMetrics = m ?? null; syncPolling(); notify(); });
    }
    win?.addEventListener?.(V3_EVENT, onV3);
    syncPolling();
  }
  function stop() {
    try { offMetrics?.(); } catch { /* Kernel weg */ }
    offMetrics = null;
    lastMetrics = null;
    win?.removeEventListener?.(V3_EVENT, onV3);
    if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  }

  const api = {
    getOverallState,
    getAppState,
    get canReconnect() { return !!owner; },
    subscribe(fn, { app = false } = {}) {
      if (destroyed || typeof fn !== 'function') return () => {};
      const l = { fn, app };
      const first = listeners.size === 0;
      if (first) { lastSig = JSON.stringify(getOverallState()); start(); }
      listeners.add(l);
      try { fn(getOverallState()); } catch (e) { console.warn('[sync-state]', e); }
      return () => { if (listeners.delete(l) && !listeners.size) stop(); };
    },
    async reconnect() {
      if (destroyed || !owner) return false;
      try { await owner.reconnect(); } catch (e) { console.warn('[sync-state] reconnect', e); return false; }
      notify(true);
      return true;
    },
    setRequested(names) { shellRequested = names; notify(); },
    refresh() { notify(true); },
    destroy() {
      destroyed = true;
      listeners.clear();
      stop();
      if (latest === api) latest = null;
    },
  };
  latest = api;
  return api;
}

export const levelRank = (level) => RANK[level] ?? RANK.unknown;
