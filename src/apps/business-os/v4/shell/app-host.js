// Shell v4 – App-Host mit Live-Remount (Entwurf §5, §6; Architektur §10.2a Nr. 1, §16.10 „Remount“).
//
// Vertrag einer App (fest):
//   export function mount(root, api, { restored }) → { unmount(): void|Promise<void>, snapshot?(): unknown }
//   mount darf ein Promise liefern. snapshot liefert structured-clone-fähige Daten ≤ 1 MiB.
//
// Zustandsfolge je Fenster:  mounting → mounted → swapping → mounted | rolled-back
//
// Swap-Ablauf (swapWindow):
//   1. snapshot() der alten Instanz; fehlt er, ist er zu groß oder nicht klonbar: generische Sicherung aller
//      input/textarea/select-Werte nach id/name. Fokus, Auswahl und Scrollpositionen sichert die Shell IMMER
//      generisch („view“), weil sie Shell-Wissen sind und keine App sie selbst zurückspielen muss.
//   2. Snapshot nach window_state (Kernel/IDB) – vor jedem Umschalten.
//   3. Neues CSS laden (versionierter <link>), erst nach dessen load das alte entfernen.
//      Zusätzlich wird das Modul der neuen Version VOR dem Abbau importiert: scheitert schon der Import, läuft die
//      alte Instanz ungestört weiter (Rückfall ohne Remount).
//   4. unmount() alt, frischer root, mount() neu mit { restored }. Kommt mount nicht binnen mountTimeoutMs (9 s)
//      zurück oder wirft es: alte Version mit demselben Snapshot wieder montieren, Zustand rolled-back, Fehler in
//      die Zustandszone, kernel.activation.reportFailed(appId, ver, reason). Gesamter Rückfall ≤ 10 s.
//      Ein verspätet doch noch fertiges mount wird sofort wieder abgebaut.
//
// Das Fenster-Chrome (Icon, Titel, Kopfzeilen-Slot, Steuerung, Zonen-Platz) gehört dem RAHMEN (v4/frame/index.js).
// Dieser Host rendert nur den App-Inhalt in root und meldet Zone-Einträge an status-zone.js, das in den vom Rahmen
// bereitgestellten Zonen-Platz rendert:
//
//   createWindowHost({ root, appRecord: {id, ver, entry?, title?}, api, restored })
//     → { swap(nextVer, meta?), snapshot(), destroy({keepState}?), activation(ev), route(params), refocus(),
//         ready, ver, state, zone, instance }
//   api (vom Rahmen): { kernel (adaptKernel), windowId, windowEl, zoneSlot, shell: {setTitle, headerSlot, close,
//         navigate, openSource, window}, shellStatus (createShellStatus), onEvent, onRolledBack, mountTimeoutMs? }
//   api.shell.assetUrl(appId, path) der App: Artefakt-URL der aktiven Version einer beliebigen App (appAssetUrl unten).
//   Bindung je Mount (§16.39, K-1d): jeder Ladevorgang einer Version (Mount, Swap, Rückfall) ruft genau einmal
//   kernel.artifacts.bind(appId, ver); Einstieg, styles und host.assetUrl(path) laufen über diese eingefrorene Bindung.
//   Die Shell gibt nie eine Shell-Version mit.
//
// Kernel-Bezug (Stub 67229d2, §16.19): Zustandseinträge der App gehen über api.status an den Kernel und kommen als
// Strom kernel.status.forApp(appId) zurück (Quellen app und kernel); eigene Einträge des Hosts (Rückfall, Mount,
// Snapshot) bleiben fensterlokal. Fensterzustand unter kernel.windowState 'mount:<fenster>'. Rückfall meldet
// kernel.activation.reportFailed, der Kernel setzt daraufhin den Aktivierungszeiger zurück.

import { createStatusZone, createActivationFilter, createMetricsWatcher } from './status-zone.js';
import { createColumnResizers } from './column-resizers.js';
import { createWindowSheet, cancelValueOf } from '../frame/window-sheet.js';
import { sendContextChat, currentSelectionText, CONTEXT_CHAT_OWNER } from '../../shared/context-chat.js';

export const MOUNT_TIMEOUT_MS = 9000;
export const SNAPSHOT_MAX_BYTES = 1024 * 1024;
const GENERIC_KIND = 'ctox.generic-snapshot/1';

function ctoxError(code, message, extra = {}) {
  return Object.assign(new Error(message), { name: 'CtoxError', code, ...extra });
}

// api.shell.openSource(appId, options): zweites Argument → { tab, reportId } für die Code-Ansicht (Strang LOOP Runde 2).
const SOURCE_TABS = ['files', 'reports'];
export function sourceFocusOf(options) {
  if (options == null) return null;
  if (typeof options !== 'object' || Array.isArray(options)) throw ctoxError('invalid', 'openSource: options muss ein Objekt sein', { path: 'options' });
  const extra = Object.keys(options).filter((k) => k !== 'tab' && k !== 'reportId');
  if (extra.length) throw ctoxError('invalid', `openSource: unbekannte Option ${extra[0]}`, { path: `options.${extra[0]}` });
  const { tab, reportId } = options;
  if (tab !== undefined && !SOURCE_TABS.includes(tab)) throw ctoxError('invalid', `openSource: tab muss ${SOURCE_TABS.join(' | ')} sein`, { path: 'options.tab' });
  if (reportId !== undefined && (typeof reportId !== 'string' || reportId.trim() === '')) throw ctoxError('invalid', 'openSource: reportId muss ein nicht leerer Text sein', { path: 'options.reportId' });
  if (tab === undefined && reportId === undefined) return null;
  return { tab: tab ?? 'reports', ...(reportId !== undefined ? { reportId } : {}) };
}

const clock = () => performance.timeOrigin + performance.now();
const validAction = (a) => !!a && typeof a.run === 'function' && String(a.label ?? '').trim() !== '';

// ---------------------------------------------------------------------------------------------------------------
// CSS je App-Version mit Referenzzählung (mehrere Fenster derselben Version teilen einen <link>).
// ---------------------------------------------------------------------------------------------------------------
const cssRegistry = new Map(); // href → {link, refs, ready}
function acquireCss(href) {
  let rec = cssRegistry.get(href);
  if (!rec) {
    const link = document.createElement('link');
    link.rel = 'stylesheet';
    link.href = href;
    link.dataset.appCss = '';
    const ready = new Promise((resolve, reject) => {
      link.onload = () => resolve();
      link.onerror = () => reject(new Error(`CSS ${href} nicht ladbar`));
    });
    document.head.append(link);
    rec = { link, refs: 0, ready };
    cssRegistry.set(href, rec);
  }
  rec.refs++;
  return rec.ready;
}
function releaseCss(href) {
  const rec = cssRegistry.get(href);
  if (!rec) return;
  if (--rec.refs <= 0) { rec.link.remove(); cssRegistry.delete(href); }
}

// ---------------------------------------------------------------------------------------------------------------
// Generische Sicherung (Formularwerte) und View-Sicherung (Fokus, Auswahl, Scroll) nach id/name.
// ---------------------------------------------------------------------------------------------------------------
function keyOf(el, root) {
  if (el === root) return ':root';
  if (el.id) return `#${el.id}`;
  const name = el.getAttribute && el.getAttribute('name');
  if (!name) return null;
  // Radiogruppen teilen den Namen: je Option eigener Schlüssel über den Wert.
  if ((el.type || '').toLowerCase() === 'radio') return `[name="${name}"][value="${el.value}"]`;
  return `[name="${name}"]`;
}
function findByKey(root, key) {
  if (key === ':root') return root;
  try { return root.querySelector(key); } catch { return null; }
}
export function captureGeneric(root) {
  const fields = {};
  for (const el of root.querySelectorAll('input, textarea, select')) {
    const key = keyOf(el, root);
    if (!key) continue;
    const type = (el.type || '').toLowerCase();
    if (type === 'password' || type === 'file' || el.dataset.noSnapshot !== undefined) continue; // nie Geheimnisse nach IDB
    if (type === 'checkbox' || type === 'radio') fields[key] = { checked: el.checked };
    else if (el.tagName === 'SELECT' && el.multiple) fields[key] = { values: [...el.selectedOptions].map((o) => o.value) };
    else fields[key] = { value: el.value };
  }
  return { kind: GENERIC_KIND, fields };
}
export function captureView(root) {
  const view = { focus: null, scroll: {} };
  const active = document.activeElement;
  if (active && active !== root && root.contains(active)) {
    const key = keyOf(active, root);
    if (key) {
      view.focus = { key };
      try {
        if (typeof active.selectionStart === 'number') {
          view.focus.selection = [active.selectionStart, active.selectionEnd, active.selectionDirection || 'none'];
        }
      } catch { /* Feldtyp ohne Auswahl */ }
    }
  }
  const scrollables = [root, ...root.querySelectorAll('[id], [name]')];
  for (const el of scrollables) {
    if (el.scrollTop || el.scrollLeft) {
      const key = keyOf(el, root);
      if (key) view.scroll[key] = [el.scrollTop, el.scrollLeft];
    }
  }
  return view;
}
export function restoreGeneric(root, generic) {
  if (!generic || generic.kind !== GENERIC_KIND) return 0;
  let n = 0;
  for (const [key, v] of Object.entries(generic.fields || {})) {
    const el = findByKey(root, key);
    if (!el) continue;
    if ('checked' in v) el.checked = v.checked;
    else if ('values' in v) for (const o of el.options) o.selected = v.values.includes(o.value);
    else el.value = v.value;
    // Die App soll den Wert wie eine Eingabe sehen (eigene Zustände, Validierung).
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
    n++;
  }
  return n;
}
export function restoreView(root, view) {
  if (!view) return;
  if (view.focus) {
    const el = findByKey(root, view.focus.key);
    if (el && typeof el.focus === 'function') {
      el.focus({ preventScroll: true });
      if (view.focus.selection && typeof el.setSelectionRange === 'function') {
        try { el.setSelectionRange(...view.focus.selection); } catch { /* Feldtyp ohne Auswahl */ }
      }
    }
  }
  // Scroll NACH Fokus/Auswahl: setSelectionRange scrollt sonst das Feld zur Einfügemarke.
  for (const [key, [top, left]] of Object.entries(view.scroll || {})) {
    const el = findByKey(root, key);
    if (el) { el.scrollTop = top; el.scrollLeft = left; }
  }
}
function sizeOf(value) {
  return new TextEncoder().encode(JSON.stringify(value) ?? '').length;
}


// ---------------------------------------------------------------------------------------------------------------
// Shell-weite Einträge (Verbindung, Outbox, Rückstand): erscheinen in der Zone JEDES offenen Fensters.
// ---------------------------------------------------------------------------------------------------------------
export function createShellStatus() {
  const entries = new Map();
  const zones = new Set();
  return {
    report(item) {
      const it = { ...item, source: 'shell-wide' };
      entries.set(it.code, it);
      for (const z of zones) z.report(it);
    },
    clear(code) {
      if (!entries.delete(code)) return;
      for (const z of zones) z.clear(code);
    },
    attach(zone) {
      zones.add(zone);
      for (const it of entries.values()) zone.report(it);
      return () => zones.delete(zone);
    },
    entries: () => [...entries.values()],
  };
}

// Eine Umgebung je laufender Shell: Shell-weite Zone plus Messwert-Wächter.
export function createHostEnvironment(kernel) {
  const status = createShellStatus();
  const watcher = kernel?.metrics?.subscribe ? createMetricsWatcher(kernel.metrics, status) : null;
  return { status, destroy() { watcher?.destroy(); } };
}

// ---------------------------------------------------------------------------------------------------------------
// Befehls-Lebenszyklus in der Zone (§16.31; Endzustände nach business-command-lifecycle-v2.json).
// Der Host beobachtet jeden Befehl, den eine App über api.command auslöst, bis zu genau EINEM Endzustand:
//   done/completed → still (Erfolg erzeugt keinen Text und räumt nichts auf), cancelled → still,
//   failed (Ausführung) → ein Fehler-Eintrag „command:<id>“ in der Zone der auslösenden App,
//   rejected (Schreibablehnung) → zeigt der Kernel selbst (write.rejected, Quelle kernel), sonst die Shell.
// Regeln: der erste Endzustand rastet ein (kein Rückwärtsübergang, Wiederzustellungen wirkungslos); Endzustände
// werden je App in client_seq-Folge freigegeben (n+1 wartet auf n, höchstens COMMAND_HOLD_MS); ein Code je Befehl
// (Upsert, nie doppelt). Das Register hängt am Kernel, nicht an der App-Instanz: Remount und Shell-Tausch beobachten
// denselben Befehl nicht ein zweites Mal. Mit Kernel-Zustandsstrom geht der Eintrag dorthin (überlebt den
// Shell-Tausch), sonst in ein Register, an das sich jede Zone der App hängt.
// ---------------------------------------------------------------------------------------------------------------
export const COMMAND_HOLD_MS = 15000;
const COMMAND_CODE = 'command:';
const TERMINAL_OF = { done: 'done', completed: 'done', failed: 'failed', cancelled: 'cancelled', canceled: 'cancelled', rejected: 'rejected' };
const commandRegistries = new WeakMap();

function terminalOf(st) {
  if (!st || typeof st !== 'object') return null;
  const raw = st.terminal_status && st.terminal_status !== 'none' ? st.terminal_status : st.status;
  const t = TERMINAL_OF[String(raw ?? '')] || null;
  if (t === 'failed' && st.error_code === 'cancelled') return 'cancelled';
  return t;
}
const seqOf = (v) => (Number.isFinite(Number(v)) && v !== null && v !== '' ? Number(v) : null);

export function commandLifecycle(kernel) {
  let reg = commandRegistries.get(kernel);
  if (reg) return reg;
  const sink = kernel?.status?.report && kernel?.status?.forApp ? kernel.status : null;
  const byId = new Map(); // command id → rec (beobachtet oder freigegeben)
  const order = new Map(); // appId → [rec] nach seq, noch nicht freigegeben
  const local = new Map(); // appId → Map(code → item)   (nur ohne Kernel-Strom)
  const zones = new Map(); // appId → Set(zone)
  let issued = 0;

  // Der Eintrag eines Befehls ist eine Funktion seines (einen) Endzustands: failed bzw. rejected ohne Kernel-Strom →
  // Fehler-Eintrag; done und cancelled → kein Eintrag dieses Befehls (still; Einträge anderer Befehle, auch auf
  // demselben Ziel, bleiben unberührt).
  function show(rec) {
    const code = `${COMMAND_CODE}${rec.id}`;
    const visible = rec.outcome === 'failed' || (rec.outcome === 'rejected' && !sink);
    const m = local.get(rec.appId);
    if (!visible) {
      if (sink) { try { sink.clear(rec.appId, code); } catch { /* Kernel weg */ } return; }
      if (m?.delete(code)) for (const z of zones.get(rec.appId) || []) z.clear(code);
      return;
    }
    const verb = rec.outcome === 'failed' ? 'fehlgeschlagen' : 'abgelehnt';
    const item = { level: 'error', code, text: `Befehl „${rec.type}“ ${verb}${rec.error ? `: ${rec.error}` : ''}`, ref: rec.id };
    if (sink) { try { sink.report(rec.appId, item); } catch (e) { console.warn('[command] status', e); } return; }
    if (!m) local.set(rec.appId, new Map());
    local.get(rec.appId).set(code, { ...item, source: 'shell' });
    for (const z of zones.get(rec.appId) || []) z.report({ ...item, source: 'shell' });
  }
  function release(appId) {
    const list = order.get(appId);
    if (!list) return;
    while (list.length && list[0].outcome) {
      const rec = list.shift();
      clearTimeout(rec.hold);
      rec.released = true;
      show(rec);
    }
    if (!list.length) { order.delete(appId); return; }
    // Kopf läuft noch, Spätere sind fertig: höchstens COMMAND_HOLD_MS warten, dann Kopf aus der Folge nehmen.
    const head = list[0];
    if (!head.hold && list.some((r) => r.outcome)) {
      head.hold = setTimeout(() => {
        const i = list.indexOf(head);
        if (i >= 0 && !head.outcome) { list.splice(i, 1); head.unordered = true; release(appId); }
      }, COMMAND_HOLD_MS);
    }
  }
  function settle(rec, outcome, error) {
    if (rec.outcome) return; // erster Endzustand rastet ein (kein Rückwärtsübergang, Wiederzustellung wirkungslos)
    rec.outcome = outcome;
    rec.error = error ? String(error) : '';
    rec.off?.();
    rec.off = null;
    if (rec.unordered || rec.released) { rec.released = true; show(rec); return; }
    release(rec.appId);
  }
  function observe(rec, h, st) {
    if (rec.outcome || rec.classifying) return;
    const s = seqOf(st?.client_seq);
    if (s !== null && s !== rec.seq && !rec.unordered) {
      rec.seq = s;
      order.get(rec.appId)?.sort((a, b) => a.seq - b.seq);
    }
    const t = terminalOf(st);
    if (!t) return; // Zwischenzustände zeigt die Zone nicht
    if (t !== 'failed' || st.error_code || !h.result) { settle(rec, t, st.error_message ?? st.error); return; }
    // „failed“ ohne Fehlercode: das Schreibergebnis unterscheidet Ablehnung/Rücknahme von Ausführungsfehlern.
    rec.classifying = true;
    Promise.race([Promise.resolve(h.result), new Promise((r) => setTimeout(() => r(null), 2000))]).then((r) => {
      rec.classifying = false;
      if (r?.status === 'rejected') settle(rec, r.reason === 'cancelled' ? 'cancelled' : 'rejected', r.detail || r.reason);
      else settle(rec, 'failed', st.error_message ?? st.error);
    }, () => { rec.classifying = false; settle(rec, 'failed', st.error); });
  }

  reg = {
    track(appId, type, h, opts) {
      const id = h?.id;
      if (!id || byId.has(id)) return;
      issued++;
      const rec = {
        id: String(id), appId, type: String(type), target: opts?.record_id ?? null,
        seq: seqOf(h.client_seq) ?? issued, outcome: null, error: '', off: null, hold: null, released: false,
      };
      byId.set(rec.id, rec);
      if (byId.size > 2000) byId.delete(byId.keys().next().value); // begrenzt; ältester ist längst freigegeben
      let list = order.get(appId);
      if (!list) order.set(appId, (list = []));
      list.push(rec);
      list.sort((a, b) => a.seq - b.seq);
      Promise.resolve(h.result).then((r) => {
        if (r?.status === 'rejected') settle(rec, r.reason === 'cancelled' ? 'cancelled' : 'rejected', r.detail || r.reason);
      }, () => {});
      if (h.status?.subscribe) {
        const off = h.status.subscribe((st) => observe(rec, h, st));
        if (rec.outcome) off(); else rec.off = off;
      }
    },
    // Ohne Kernel-Strom: Zone der App anhängen (spielt vorhandene Einträge ein).
    attach(appId, zone) {
      if (sink) return () => {};
      let set = zones.get(appId);
      if (!set) zones.set(appId, (set = new Set()));
      set.add(zone);
      for (const it of local.get(appId)?.values() || []) zone.report(it);
      return () => set.delete(zone);
    },
    pending: () => [...byId.values()].filter((r) => !r.released).map((r) => ({ id: r.id, appId: r.appId, seq: r.seq, outcome: r.outcome })),
  };
  commandRegistries.set(kernel, reg);
  return reg;
}

// ---------------------------------------------------------------------------------------------------------------
// api.shell.assetUrl: Artefakt-URL der aktiven Version einer App (Form: sw.js URL_RE).
// ---------------------------------------------------------------------------------------------------------------
// Die URL baut der Kernel (kernel.artifacts.url, §16.39 Nr. 1, K-1d): ohne Bindung mit der aktiven Shell, er merkt sich
// nichts. Der Host wählt nur die aktive Version. Ergebnis muss eine Zeichenkette sein (SW-Weg); alles andere → null.
export function appAssetUrl(kernel, appId, path) {
  const id = typeof appId === 'string' ? appId : '';
  const rel = typeof path === 'string' ? path.replace(/^\.?\/+/, '') : '';
  if (!id || !rel) return null;
  let rows;
  try { rows = kernel?.apps?.get?.(); } catch { rows = null; }
  const row = Array.isArray(rows) ? rows.find((r) => r?.id === id) : null;
  const ver = row?.activated_ver;
  if (ver == null || ver === '') return null;
  let url = null;
  try { url = kernel?.artifacts?.url?.(id, String(ver), rel) ?? null; } catch { url = null; }
  return typeof url === 'string' ? url : null;
}

// ---------------------------------------------------------------------------------------------------------------
// Fenster-Host: genau eine App in genau einem Rahmen-Fenster.
// ---------------------------------------------------------------------------------------------------------------
export function createWindowHost({ root, appRecord, api = {}, restored = null }) {
  const kernel = api.kernel;
  if (!kernel?.artifacts) throw new Error('createWindowHost: api.kernel (adaptKernel) fehlt');
  const appId = String(appRecord?.id || '');
  const frame = api.shell || {};
  const mountTimeoutMs = api.mountTimeoutMs ?? MOUNT_TIMEOUT_MS;
  const onEvent = typeof api.onEvent === 'function' ? api.onEvent : () => {};
  const win = {
    id: String(api.windowId || appId),
    appId,
    ver: String(appRecord?.ver || ''),
    state: 'mounting',
    instance: null,
    styles: [],
    binding: null, // kernel.artifacts.bind des aktuell geladenen Mounts
    queue: Promise.resolve(),
    failed: new Set(), // Versionen, die in diesem Fenster zurückgefallen sind (kein Wiederholungs-Takt)
    body: root,
    el: api.windowEl || root.parentElement || root,
    headerSlot: frame.headerSlot || null,
    destroyed: false,
    lastView: null,
    // Start-Route (navigate auf eine noch geschlossene App): {from, to, params}. Geht als ctx.route an mount und als
    // erstes onRoute-Ereignis (dasselbe Objekt) an den ersten onRoute-Abonnenten, auch wenn er sich erst nach mount
    // anmeldet. Danach verbraucht; ein Remount stellt sie nicht erneut zu.
    pendingRoute: api.params && typeof api.params === 'object'
      ? Object.freeze({ from: String(api.routeFrom || 'shell'), to: appId, params: api.params })
      : null,
    mountRoute: null,
  };
  win.mountRoute = win.pendingRoute;
  const stateKey = () => `mount:${win.id}`; // je Tab und Nutzer trennt der Kernel (§16.19 Nr. 3)
  const emit = (extra = {}) => onEvent({ type: 'state', windowId: win.id, appId, ver: win.ver, state: win.state, at: clock(), ...extra });
  const setState = (state, extra) => {
    win.state = state;
    win.el.dataset.hostState = state;
    emit(extra);
  };

  let zoneSlot = api.zoneSlot;
  if (!zoneSlot) { zoneSlot = document.createElement('div'); root.after(zoneSlot); }
  const zone = createStatusZone(zoneSlot, win.el);
  win.zone = zone;
  // FR2-3: Spaltenteiler im Inhaltsbereich, Breiten je Fenster und App in kernel.windowState.
  const cols = createColumnResizers({ appId, windowId: win.id, windowState: kernel.windowState });
  win.cols = cols;
  // Aktionen der App-Einträge (item.action {label, run}): Funktionen laufen nicht über den Kernel-Strom,
  // der Host hält sie je Code und hängt sie an den zurückkommenden Eintrag.
  const appActions = new Map(); // code (ohne Präfix) → action
  // api.shell.dialog/menu: Blatt bzw. Menü im Fensterelement (v4/frame/window-sheet.js), modal nur für dieses Fenster.
  const sheet = createWindowSheet({ host: win.el, inertTargets: win.el === root ? null : () => [root, zoneSlot], onOpen: () => frame.focus?.() });
  win.sheet = sheet;
  const offContext = bindContextMenu();
  win.activationFilter = createActivationFilter(zone);
  const detachShellStatus = api.shellStatus?.attach?.(zone) || null;
  const commands = commandLifecycle(kernel);
  const detachCommands = commands.attach(appId, zone);
  // §16.19 Nr. 1: der Kernel liefert die Zustandseinträge als Strom, rendern macht die Shell. Gerendert werden die
  // Quellen app (api.status.report) und kernel (Ablehnungen, Aktivierung); source 'shell' sind Einträge anderer
  // Shell-Teile über kernel.status.report, die eigenen Einträge dieses Hosts stehen direkt in der Zone.
  let offKernelZone = null;
  const kernelStatus = kernel.status?.forApp ? kernel.status : null;
  if (kernelStatus) {
    let shown = new Set();
    offKernelZone = kernelStatus.forApp(appId).subscribe((rows) => {
      const next = new Set();
      for (const e of rows || []) {
        // source 'shell' nur für Befehls-Endzustände (commandLifecycle), die anderen Shell-Einträge stehen direkt in der Zone.
        if (e.source !== 'kernel' && e.source !== 'app' && !(e.source === 'shell' && String(e.code).startsWith(COMMAND_CODE))) continue;
        // Den eigenen Rückfall erklärt shell.rollback („… läuft weiter“); die Kernel-Meldung zu reportFailed ist doppelt.
        if (e.source === 'kernel' && e.code === 'app.activation_failed' && e.ref != null && win.failed.has(String(e.ref))) continue;
        const code = `${e.source}:${e.code}`;
        next.add(code);
        const action = e.source === 'app' ? appActions.get(e.code) : null;
        // dismiss räumt den Kernel-Eintrag der App: nach erfolgreicher eigener Aktion und nach Ablauf eines info (6 s).
        zone.report({ level: e.level, code, text: e.text, ref: e.ref, source: e.source, action, dismiss: e.source === 'app' ? () => { appActions.delete(e.code); win.instance?.api?.status?.clear?.(e.code); } : null });
      }
      for (const c of shown) if (!next.has(c)) zone.clear(c);
      shown = next;
    });
  }

  // Generisches Kontextmenü (Rechtsklick, Shift+F10, Kontextmenü-Taste) im Fenster. Ziel = nächstes Element mit
  // data-context-record-id (bzw. data-context-id) im App-root; Typ data-context-record-type, Name data-context-label.
  // Standardeinträge NUR, wenn die App sie anbietet: data-context-actions="open edit rename delete" (Leerzeichen oder
  // Komma). Die Wahl geht als DOM-Ereignis 'ctox:context-action' {action, record} (bubbles) an das Datensatz-Element;
  // die App führt sie aus. Immer dabei: „CTOX fragen…“ → Eingabeblatt → Standardbefehl ctox.chat.task
  // (shared/context-chat.js) unter der App-Identität. Hat die App den Rechtsklick selbst behandelt
  // (preventDefault), bleibt der Rahmen still; ohne Datensatz in Eingabefeldern oder mit Textauswahl bleibt das
  // Browsermenü (Kopieren/Einfügen).
  function bindContextMenu() {
    const ACTIONS = [['open', 'Öffnen'], ['edit', 'Bearbeiten'], ['rename', 'Umbenennen'], ['delete', 'Löschen']];
    const recordOf = (el) => {
      if (!el) return null;
      const id = el.getAttribute('data-context-record-id') || el.getAttribute('data-context-id') || '';
      if (!id) return null;
      const label = el.getAttribute('data-context-label') || el.getAttribute('data-context-record-label')
        || String(el.getAttribute('aria-label') || el.textContent || '').trim().split('\n')[0].slice(0, 80).trim();
      return { id, type: el.getAttribute('data-context-record-type') || el.getAttribute('data-record-type') || appId, label };
    };
    let keyboardAt = 0;
    async function open(target, at, viaKeyboard) {
      const appRoot = win.instance?.root;
      if (!appRoot || !appRoot.contains(target)) return false;
      const recEl = target.closest?.('[data-context-record-id], [data-context-id]');
      const recordEl = recEl && appRoot.contains(recEl) ? recEl : null;
      const record = recordOf(recordEl);
      const selection = currentSelectionText();
      if (!record && !viaKeyboard && (selection || target.closest?.('input, textarea, select, [contenteditable=""], [contenteditable="true"]'))) return false;
      const offered = new Set(String(recordEl?.getAttribute('data-context-actions') || '').toLowerCase().split(/[\s,]+/).filter(Boolean));
      const items = [];
      for (const [id, label] of ACTIONS) {
        if (!offered.has(id)) continue;
        if (id === 'delete' && items.length) items.push({ type: 'separator' });
        items.push({ id: `record.${id}`, label, danger: id === 'delete' });
      }
      if (items.length) items.push({ type: 'separator' });
      items.push({ id: 'ctox.ask', label: 'CTOX fragen…' });
      const choice = await sheet.menu({ items, at, label: record?.label || 'Kontextmenü' });
      if (!choice) return true;
      if (choice.startsWith('record.')) {
        recordEl?.dispatchEvent(new CustomEvent('ctox:context-action', { bubbles: true, cancelable: true, detail: { action: choice.slice(7), record } }));
        return true;
      }
      const inst = win.instance;
      const text = await sheet.dialog({
        kind: 'prompt',
        title: 'CTOX fragen',
        text: record?.label ? `Zu „${record.label}“` : '',
        placeholder: 'Frage oder Auftrag',
        confirm_label: 'Senden',
        required: true,
        default: null,
      });
      if (text == null || !String(text).trim() || !inst || win.instance !== inst) return true;
      try {
        sendContextChat(inst.api, {
          source_app: appId,
          record_ref: record?.id || '',
          selection,
          text,
          mode: 'ask',
          title: record?.label || '',
          context: record ? { record_type: record.type, record_id: record.id, label: record.label } : {},
        }, { navigate: false });
        let rows = null;
        try { rows = kernel.apps?.get?.(); } catch { rows = null; }
        if (Array.isArray(rows) && rows.some((r) => r?.id === CONTEXT_CHAT_OWNER && r.activated_ver != null)) frame.navigate?.(CONTEXT_CHAT_OWNER, { source_app: appId, mode: 'ask' });
      } catch (err) {
        zone.report({ level: 'error', code: 'shell.context', text: `Auftrag an CTOX nicht gesendet: ${err?.message || err}`, source: 'shell' });
      }
      return true;
    }
    const onContext = (ev) => {
      if (ev.defaultPrevented || win.destroyed || sheet.isOpen()) { if (sheet.isOpen()) ev.preventDefault(); return; }
      if (performance.now() - keyboardAt < 300) { ev.preventDefault(); return; } // schon über die Tastatur geöffnet
      const target = ev.target instanceof Element ? ev.target : ev.target?.parentElement;
      if (!target || !win.instance?.root?.contains(target)) return;
      const recEl = target.closest('[data-context-record-id], [data-context-id]');
      const editable = target.closest('input, textarea, select, [contenteditable=""], [contenteditable="true"]');
      if (!recEl && (editable || currentSelectionText())) return;
      ev.preventDefault();
      void open(target, { x: ev.clientX, y: ev.clientY }, false);
    };
    const onKey = (ev) => {
      if (!((ev.shiftKey && ev.key === 'F10') || ev.key === 'ContextMenu')) return;
      if (ev.defaultPrevented || win.destroyed || sheet.isOpen()) return;
      const target = ev.target instanceof Element ? ev.target : null;
      if (!target || !win.instance?.root?.contains(target)) return;
      ev.preventDefault();
      keyboardAt = performance.now();
      const r = (target.closest('[data-context-record-id], [data-context-id]') || target).getBoundingClientRect();
      void open(target, { x: Math.round(r.left + 8), y: Math.round(r.top + Math.min(r.height, 24)) }, true);
    };
    // Ein Zeigerdruck ist nie das Folgeereignis der Tastatur: die Sperre gegen das doppelte Öffnen endet hier.
    const onPointer = () => { keyboardAt = 0; };
    root.addEventListener('contextmenu', onContext);
    root.addEventListener('keydown', onKey);
    root.addEventListener('pointerdown', onPointer, true);
    return () => {
      root.removeEventListener('contextmenu', onContext);
      root.removeEventListener('keydown', onKey);
      root.removeEventListener('pointerdown', onPointer, true);
    };
  }

  // App-API für genau eine Instanz. Nach dispose() wirkungslos; vergessene Abonnements beendet der Host.
  function buildApi(ver, appRoot) {
    const data = kernel.appApi(appId, ver);
    const subs = new Set();
    let header = null; // §16.19 Nr. 5: genau ein Kopfzeilen-Element je Instanz (Referenz, keine Kopie)
    const routes = new Set();
    let disposed = false;
    const track = (live) => live && {
      ...live,
      subscribe(cb) {
        if (disposed) return () => {};
        const off = live.subscribe((v) => { if (!disposed) cb(v); });
        subs.add(off);
        return () => { subs.delete(off); off(); };
      },
    };
    const appApi = {
      ...data,
      app: { id: appId, ver },
      query: (c, q) => track(data.query(c, q)),
      get: (c, id) => track(data.get(c, id)),
      command: (type, payload, opts) => {
        const h = data.command(type, payload, opts);
        commands.track(appId, type, h, opts); // Endzustand in die Zone, unabhängig von der Lebensdauer dieser Instanz
        return { ...h, status: track(h.status) };
      },
      metrics: track(data.metrics),
      actor: data.actor ? { ...data.actor, permissions: track(data.actor.permissions) } : data.actor,
      status: kernelStatus && data.status?.report
        ? {
          // Über den Kernel (Strom kernel.status), damit Kernel, Shell und andere Teilnehmer dieselbe Quelle sehen.
          report(item) {
            if (disposed || !item?.code) return;
            const { action, ...plain } = item;
            if (validAction(action)) appActions.set(item.code, action); else appActions.delete(item.code);
            data.status.report(plain);
          },
          clear(code) { if (disposed) return; appActions.delete(code); data.status.clear(code); },
        }
        : {
          report(item) { if (!disposed && item?.code) zone.report({ ...item, code: `app:${item.code}`, source: 'app', action: validAction(item.action) ? item.action : null }); },
          clear(code) { if (!disposed) zone.clear(`app:${code}`); },
        },
      shell: {
        ...(data.shell || {}),
        setTitle(text) { if (!disposed) frame.setTitle?.(String(text ?? '')); },
        // §16.19 Nr. 5: registerHeader(el: HTMLElement | null): void. Ein neues Element ersetzt das alte, null entfernt
        // es, alles andere ist CtoxError invalid. Die Referenz geht zusätzlich an den Kernel (kernel.shell.window).
        registerHeader(el) {
          if (el !== null && !(el instanceof HTMLElement)) throw ctoxError('invalid', 'registerHeader braucht ein HTMLElement (oder null)', { path: 'el' });
          if (disposed) return;
          if (header && header !== el) header.remove();
          header = el;
          if (el && win.headerSlot) win.headerSlot.append(el);
          try { data.shell?.registerHeader?.(el); } catch { /* Kernel ohne Fensterdaten */ }
        },
        navigate(target, params) {
          if (disposed) return false;
          const ok = !!frame.navigate?.(target, params);
          if (!ok) zone.report({ level: 'error', code: 'shell.navigate', text: `Ziel „${target}“ ist nicht vorhanden`, source: 'shell' });
          return ok;
        },
        // api.shell.openSource(appId?) → boolean: Quelltext-Ansicht einer App (Standard: die eigene) als Fenster des
        // Rahmens. Generisch für alle Apps; nur mit apps.source.view und kernel.source (entschieden im Rahmen/Kern).
        // Nicht möglich → false und genau ein Eintrag shell.openSource in der Zone dieses Fensters.
        // options (Strang LOOP Runde 2): { tab?: 'files' | 'reports', reportId?: string } – die Code-Ansicht öffnet auf
        // diesem Reiter, reportId markiert die Meldung im Reiter „Meldungen“ (reportId ohne tab → 'reports').
        // Anderes als ein Objekt mit diesen Feldern → CtoxError invalid (kein stilles Ignorieren).
        openSource(target, options) {
          if (disposed) return false;
          const id = target == null || target === '' ? appId : String(target);
          const focus = sourceFocusOf(options);
          const ok = !!frame.openSource?.(id, focus);
          if (!ok) zone.report({ level: 'error', code: 'shell.openSource', text: `Quelltext von „${id}“ ist nicht verfügbar`, source: 'shell' });
          return ok;
        },
        onRoute(cb) {
          routes.add(cb);
          const ev = win.pendingRoute;
          if (ev && !disposed) {
            win.pendingRoute = null;
            queueMicrotask(() => { if (routes.has(cb)) { try { cb(ev); } catch (e) { console.error(e); } } });
          }
          return () => routes.delete(cb);
        },
        // api.shell.assetUrl(appId, path) → versionierte Artefakt-URL der AKTIVEN Version einer (beliebigen) App, mit
        // gebundener Shell-Version: /~a/<app>@<aktive app_ver>~shell@<shell_ver>/<pfad>. Quelle sind ausschließlich
        // die Kernel-App-Einträge (kernel.apps, activated_ver); unbekannte App oder keine aktive Version → null.
        // Generisch für alle Apps, keine Sonderfälle.
        assetUrl: (targetAppId, path) => appAssetUrl(kernel, targetAppId, path),
        // Nur innerhalb des App-Fensters (App-API-Vertrag). Nach dem Abbau der Instanz: Abbruch-Antwort.
        dialog(spec) { return disposed ? Promise.resolve(cancelValueOf(spec)) : sheet.dialog(spec); },
        menu(spec) { return disposed ? Promise.resolve(null) : sheet.menu(spec); },
        locale: data.shell?.locale ?? (kernel.locale || navigator.language || 'de-DE'),
        t: data.shell?.t ?? ((key) => key),
        window: { ...(data.shell?.window || {}), ...(frame.window || {}), close: () => frame.close?.() },
      },
    };
    const dispose = () => {
      disposed = true;
      sheet.cancelAll();
      for (const off of subs) { try { off(); } catch { /* egal */ } }
      subs.clear();
      if (header) header.remove();
      header = null;
      routes.clear();
    };
    return { api: appApi, dispose, routes, root: appRoot };
  }

  async function mountVersion(ver, loaded, restoredRec) {
    const appRoot = document.createElement('div');
    appRoot.className = 'v4-app-root';
    appRoot.dataset.app = appId;
    appRoot.dataset.appVer = ver;
    win.body.replaceChildren(appRoot);
    // Fenstertitel = Titel der App, ohne Versionsnummer.
    frame.setTitle?.(loaded.manifest.title || appRecord?.title || appId);
    const ctx = buildApi(ver, appRoot);
    const value = restoredRec ? (restoredRec.app ?? restoredRec.generic ?? undefined) : undefined;
    let timer;
    let timedOut = false;
    const route = win.mountRoute || undefined;
    win.mountRoute = null;
    const mountP = Promise.resolve().then(() => loaded.mod.mount(appRoot, ctx.api, { restored: value === undefined ? undefined : structuredClone(value), route }));
    try {
      const handle = await Promise.race([
        mountP,
        new Promise((_, reject) => { timer = setTimeout(() => { timedOut = true; reject(new Error(`mount() nicht binnen ${mountTimeoutMs / 1000} s fertig`)); }, mountTimeoutMs); }),
      ]);
      clearTimeout(timer);
      if (!handle || typeof handle.unmount !== 'function') throw new Error('mount() lieferte kein { unmount }');
      await cols.ready;
      cols.attach(appRoot);
      if (restoredRec) {
        if (!restoredRec.app) restoreGeneric(appRoot, restoredRec.generic);
        restoreView(appRoot, restoredRec.view);
        win.lastView = restoredRec.view || null;
        setTimeout(() => { if (win.instance?.root === appRoot) restoreView(appRoot, restoredRec.view); }, 0);
      }
      return { ver, handle, root: appRoot, ...ctx };
    } catch (err) {
      clearTimeout(timer);
      ctx.dispose();
      appRoot.remove();
      if (timedOut) mountP.then((h) => { try { h?.unmount?.(); } catch { /* verspätet */ } }, () => {});
      throw err;
    }
  }

  async function loadModule(ver) {
    const manifest = await kernel.artifacts.manifest(appId, ver);
    if (!manifest) throw new Error(`Manifest ${appId}@${ver} fehlt`);
    // Genau EIN bind je Ladevorgang (= Mount dieser Version): Shell-Version und Lib-Minors eingefroren (§16.39, K-1d).
    const binding = kernel.artifacts.bind(appId, String(ver));
    const styles = (manifest.styles || []).map((p) => binding.url(String(p).replace(/^\.?\/+/, '')));
    await Promise.all(styles.map(acquireCss));
    try {
      const mod = await import(binding.url(String(manifest.entry || appRecord?.entry || 'index.js').replace(/^\.?\/+/, '')));
      if (typeof mod.mount !== 'function') throw new Error(`${appId}@${ver} exportiert kein mount()`);
      return { mod, styles, manifest, binding };
    } catch (err) {
      styles.forEach(releaseCss);
      throw err;
    }
  }

  async function unmountInstance(inst) {
    if (!inst) return;
    cols.detach();
    try { await Promise.race([Promise.resolve(inst.handle.unmount()), new Promise((r) => setTimeout(r, 2000))]); } catch (e) { console.warn('unmount', e); }
    inst.dispose();
    inst.root.remove();
  }

  // Snapshot: app (eigener snapshot, geprüft), generic (falls kein app), view (immer).
  async function captureSnapshot() {
    const inst = win.instance;
    const rec = { app: null, generic: null, view: null, ver: win.ver, at: clock() };
    if (!inst) return rec;
    if (typeof inst.handle.snapshot === 'function') {
      try {
        const value = await inst.handle.snapshot();
        const cloned = structuredClone(value);
        const bytes = sizeOf(cloned);
        if (bytes > SNAPSHOT_MAX_BYTES) throw new Error(`Snapshot ${bytes} B > 1 MiB`);
        rec.app = cloned === undefined ? null : cloned;
      } catch (err) {
        zone.report({ level: 'warn', code: 'shell.snapshot', text: `Zustand der App nicht vollständig gesichert: ${err.message}`, source: 'shell' });
      }
    }
    if (rec.app == null) rec.generic = captureGeneric(inst.root);
    rec.view = captureView(inst.root);
    return rec;
  }

  function rolledBack(failedVer, ver, reason) {
    win.failed.add(failedVer);
    // Kernel setzt den Aktivierungszeiger zurück (§16.19 Nr. 2); alle Tabs sehen activated_ver = letzte gute Version.
    Promise.resolve(kernel.activation?.reportFailed?.(appId, failedVer, reason)).catch((e) => console.warn('reportFailed', e));
    try { api.onRolledBack?.({ appId, windowId: win.id, failedVer, ver, reason }); } catch { /* Rahmen-Rückmeldung optional */ }
  }

  // Erstmontage
  win.ready = (async () => {
    try {
      const loaded = await loadModule(win.ver);
      try {
        win.instance = await mountVersion(win.ver, loaded, restored);
        win.styles = loaded.styles;
        win.binding = loaded.binding;
        setState('mounted');
      } catch (err) {
        loaded.styles.forEach(releaseCss);
        throw err;
      }
    } catch (err) {
      if (win.destroyed) return;
      // Kein Vorgänger zum Zurückfallen: Fenster leer, Fehler in der Zone.
      zone.report({ level: 'error', code: 'shell.mount', text: `App konnte nicht gestartet werden: ${err.message}`, source: 'shell' });
      win.failed.add(win.ver);
      Promise.resolve(kernel.activation?.reportFailed?.(appId, win.ver, err.message)).catch(() => {});
      setState('failed', { error: err.message });
    }
  })();
  win.queue = win.ready;

  async function doSwap(newVer, meta) {
    if (win.destroyed) return;
    if (win.ver === newVer && win.state !== 'failed') {
      if (meta.explicit && !meta.reverted) {
        zone.clear('shell.activation');
        if (win.state === 'rolled-back') { zone.clear('shell.rollback'); setState('mounted', meta); }
      }
      return;
    }
    if (win.failed.has(newVer)) {
      if (!meta.explicit || meta.reverted) return; // bekannte kaputte Version: nicht im Takt erneut versuchen
      win.failed.delete(newVer);
    }
    const oldVer = win.ver;
    const oldStyles = win.styles;
    const hadInstance = !!win.instance;
    const ph = { start: clock() };
    setState('swapping', meta);
    const rec = await captureSnapshot();
    ph.snapshot = clock();
    try { await kernel.windowState.put(stateKey(), { appId, ver: oldVer, rec }); } catch (e) { console.warn('window_state', e); }
    ph.persist = clock();
    let loaded;
    try {
      loaded = await loadModule(newVer);
    } catch (err) {
      zone.report({ level: 'error', code: 'shell.rollback', text: `Version ${newVer} nicht ladbar, ${oldVer} bleibt aktiv: ${err.message}`, source: 'shell' });
      setState(hadInstance ? 'rolled-back' : 'failed', { ...meta, error: err.message, failedVer: newVer });
      rolledBack(newVer, oldVer, err.message);
      return;
    }
    ph.load = clock();
    oldStyles.forEach(releaseCss);
    await unmountInstance(win.instance);
    win.instance = null;
    ph.unmount = clock();
    try {
      win.instance = await mountVersion(newVer, loaded, rec);
      win.ver = newVer;
      win.styles = loaded.styles;
      win.binding = loaded.binding;
      zone.clear('shell.rollback');
      zone.clear('shell.mount');
      ph.mount = clock();
      const phases = { snapshot: ph.snapshot - ph.start, persist: ph.persist - ph.snapshot, load: ph.load - ph.persist, unmount: ph.unmount - ph.load, mount: ph.mount - ph.unmount };
      setState('mounted', { ...meta, phases });
    } catch (err) {
      loaded.styles.forEach(releaseCss);
      if (!hadInstance) {
        zone.report({ level: 'error', code: 'shell.mount', text: `App konnte nicht gestartet werden: ${err.message}`, source: 'shell' });
        win.ver = newVer;
        setState('failed', { ...meta, error: err.message, failedVer: newVer });
        rolledBack(newVer, oldVer, err.message);
        return;
      }
      // Rückfall: alte Version mit demselben Snapshot.
      const back = await loadModule(oldVer);
      win.instance = await mountVersion(oldVer, back, rec);
      win.styles = back.styles;
      win.binding = back.binding;
      win.ver = oldVer;
      zone.report({ level: 'error', code: 'shell.rollback', text: `Version ${newVer} startet nicht (${err.message}); ${oldVer} läuft weiter`, source: 'shell' });
      setState('rolled-back', { ...meta, error: err.message, failedVer: newVer });
      rolledBack(newVer, oldVer, err.message);
    }
  }

  return {
    win,
    zone,
    ready: win.ready,
    get ver() { return win.ver; },
    // Eigene Datei dieses Mounts über seine Bindung (gleiche Wurzel wie der geladene Code); vor dem Laden null.
    assetUrl(path) { try { return win.binding ? win.binding.url(String(path || '').replace(/^\.?\/+/, '')) : null; } catch { return null; } },
    get state() { return win.state; },
    get instance() { return win.instance; },
    swap(nextVer, meta = {}) {
      if (!nextVer) return win.queue;
      win.queue = win.queue.then(() => doSwap(String(nextVer), meta)).catch((e) => console.error('swap', e));
      return win.queue;
    },
    // Alle Aktivierungsereignisse der App (2-s-Regel für received/fetched/data-ready, failed sofort).
    activation(ev) {
      if (win.destroyed || !ev) return;
      // Die eigene Rückfall-Meldung (reportFailed → failed) steht schon als shell.rollback in der Zone.
      if (ev.state === 'failed' && ev.ver && win.failed.has(String(ev.ver))) return;
      win.activationFilter.onActivation({ state: ev.state, ver: ev.ver, error: ev.error });
    },
    // RouteEvent nach Vertrag (Stub index.d.ts): {from, to, params}. Vorher bekam onRoute die nackten Parameter,
    // während alle portierten Apps ev.params lesen (ctox, desktop, documents …) – Deep-Links kamen nie an.
    route(params, { from = 'shell' } = {}) {
      const ev = { from: String(from || 'shell'), to: appId, params: params && typeof params === 'object' ? params : {} };
      const cbs = win.instance?.routes;
      // Noch nicht montiert oder (noch) kein Abonnent: als Start-Route vormerken statt zu verlieren.
      if (!cbs || !cbs.size) { win.pendingRoute = Object.freeze(ev); if (!win.instance) win.mountRoute = win.pendingRoute; return; }
      for (const cb of cbs) { try { cb(ev); } catch (e) { console.error(e); } }
    },
    // Fokus/Auswahl/Scroll nach dem Umschalten des Doppelpuffers erneut setzen.
    refocus() { if (win.instance && win.lastView) restoreView(win.instance.root, win.lastView); },
    async snapshot() {
      await win.queue;
      await cols.flush();
      return captureSnapshot();
    },
    columnWidths: () => cols.widths(),
    async destroy({ keepState = false } = {}) {
      if (win.destroyed) return;
      win.destroyed = true;
      await win.queue.catch(() => {});
      await unmountInstance(win.instance);
      win.instance = null;
      win.styles.forEach(releaseCss);
      win.styles = [];
      win.activationFilter.destroy();
      offKernelZone?.();
      offContext();
      sheet.destroy();
      detachShellStatus?.();
      detachCommands();
      zone.destroy();
      cols.detach();
      if (!keepState) {
        Promise.resolve(kernel.windowState.delete(stateKey())).catch(() => {});
        cols.forget();
      } else cols.flush();
    },
  };
}
