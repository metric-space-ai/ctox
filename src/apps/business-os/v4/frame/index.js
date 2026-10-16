// Shell v4 – die EINE Shell (Strang I: Rahmen F + App-Host P2 zusammengeführt).
// Einziger Einstieg für den Loader (Entwurf §3, Architektur §16.15):
//   startShell(kernel, { restored, root?, onEvent?, createWindowHost? }) -> Promise<void>
//   stopShell() -> Promise<ShellSnapshot>
// kernel = Kern-Stub, Shell-Kernel-API (§10.2a Nr. 8) oder Labor-Attrappe; ../shell/kernel-adapter.js normalisiert.
// Der Rahmen besitzt das Fenster-Chrome (Icon, Titel, Kopf-Slot für registerHeader, Steuerung, Zonen-Platz).
// Den App-Inhalt rendert ../shell/app-host.js (createWindowHost) in den Inhaltsbereich des Fensters.
// Die App-Liste kommt ausschliesslich aus kernel.apps. Kein Katalog, kein HTTP, keine Overlays,
// keine Zustandstexte ausser in der Zustandszone.
//
// Kernel-Bezug (Stub 67229d2, §16.19): App-Liste mit layout/styles aus kernel.apps (Icons, Kategorien, Fenstergrößen),
// Fensteranordnung je Tab in kernel.windowState ('shell'), Pins und Launcher-Reihenfolge nutzerweit in
// shell_preferences (eine Zeile je Nutzer, live über alle Tabs), Rückfall über kernel.activation.reportFailed.
//
// „Neue App“ (K24-Vorbedingung): Aktion im Launcher-Kopf, nur mit apps.source.edit (Spiegel aus actor.permissions der
// Shell-App-API; entschieden wird im Kern) und nur, wenn der Kernel kernel.source(appId) hat. Öffnet ein Fenster des
// Rahmens (kein Overlay) mit der Neue-App-Fläche des Quelltext-Modus (../source/upload.js openAppUpload: Ordner
// wählen → App-ID → Veröffentlichen als neuer Entwurf mit base_app_ver = null). Fehler gehen in die Zone des Fensters.
//
// „Quelltext“ (api.shell.openSource(appId?), Standardschnittstelle für alle Apps): öffnet ein Fenster des Rahmens mit
// dem Quelltext-Modus (../source/source-mode.js openSourceMode) für die App; nur mit apps.source.view und
// kernel.source, sonst false + Zoneneintrag shell.openSource im aufrufenden Fenster. Ohne apps.source.edit nur lesbar.
//
// SVG-Icons ohne xmlns (Fremd-Apps): icon_svg bekommt xmlns beim Einlesen (wie gen-manifest); ein icon_asset *.svg,
// das als Bild nicht dekodiert, wird über das Artefakt (kernel.artifacts) gelesen, um xmlns ergänzt und als data-URL
// gezeigt.
//
// Shell-Live-Tausch als Doppelpuffer: stopShell() sichert den Snapshot, meldet alle Abos ab und lässt den alten
// Rahmen sichtbar stehen (data-v4-retiring). startShell() baut den neuen Rahmen unsichtbar (opacity 0) darüber,
// montiert alle Fenster und schaltet erst danach in EINEM Schritt um: altes Root weg (Ereignis v4:retire, der alte
// Rahmen baut seine Apps danach ab), altes Shell-CSS weg, neues Root sichtbar.

import { createWindowManager } from '../../shared/window-manager.js';
import { decodeTaskbarPinCache, encodeTaskbarPinCache, resolveTaskbarPinState } from '../../shared/taskbar-pins.js';
import { createWindowHost as createAppWindowHost, createHostEnvironment } from '../shell/app-host.js';
import { adaptKernel } from '../shell/kernel-adapter.js';
import { createStatusZone, APP_MENU, attachSyncState } from '../shell/status-zone.js';
import { createSyncState } from '../shell/sync-state.js';
import { createLauncher } from './launcher.js';
import { createDesktop } from './desktop.js';
import { createCrewBar } from './crew-bar.js';
import { taskGroupOf } from './taskbar.js';
import { createTopbar } from './topbar.js';
import { installReporter, reportProblem, watchFixedReports } from '../shell/report.js';

// FR6b: Meldeschleife als Rahmen-Export (gleiche Funktion wie ../shell/report.js), siehe INTEGRATION.md.
export { reportProblem };
import { appTitle, categoryLabel } from './glossary.js';
import { createZoneGuard } from './pane-heads.js';

export const FRAME_SNAPSHOT_VERSION = 1;
const SHELL_STATE_KEY = 'shell'; // kernel.windowState: je Tab und Nutzer (§16.19 Nr. 3)
const OWNER_PREFIX = 'v4app:';
const RETIRE_EVENT = 'v4:retire';
const CHAT_APP_ID = 'ctox'; // CHAT4: Eigentümer von business_chats und Empfänger von business_os.chat.task
// Längstens so lange steht der alte Rahmen, während der neue montiert. Muss unter der Startfrist des Loaders
// (shellStartTimeoutMs 9 s, §16.19 Nr. 4) bleiben, sonst fällt der Loader auf die alte Shell zurück.
const SWITCH_DEADLINE_MS = 8500;
const PERM_SOURCE_EDIT = 'apps.source.edit';
// Shell-Ansichten (Fenster des Rahmens ohne App): app_id im Snapshot.
export const NEW_APP_VIEW = 'shell:new-app';
// Quelltext-Ansicht einer App (api.shell.openSource): app_id im Snapshot `shell:source:<appId>`.
export const SOURCE_VIEW = 'shell:source';
const PERM_SOURCE_VIEW = 'apps.source.view';
const SVG_NS = 'http://www.w3.org/2000/svg';

// Ergänzt xmlns am <svg>-Starttag, wenn er fehlt (sonst dekodiert der Browser das Bild nicht). Anderes bleibt unverändert.
export function ensureSvgXmlns(text) {
  const src = String(text || '');
  const m = /<svg\b[^>]*>/i.exec(src);
  if (!m || /\sxmlns\s*=/.test(m[0])) return src;
  return `${src.slice(0, m.index)}${m[0].replace(/^<svg\b/i, `<svg xmlns="${SVG_NS}"`)}${src.slice(m.index + m[0].length)}`;
}

// Ersatz-Icon (FR6a, M-17): ruhiger Verlauf, Farbton deterministisch aus dem app_id-Hash, darauf der Anfangsbuchstabe.
// Läuft wie jedes Icon durch die Pixel-Palette des Fensters (keine fest verdrahteten Kantenfarben).
function hashOf(text) {
  let h = 2166136261;
  for (const ch of String(text)) { h ^= ch.codePointAt(0); h = Math.imul(h, 16777619); }
  return h >>> 0;
}
const hueOf = (appId) => hashOf(appId) % 360;
export function placeholderColor(appId) { return `hsl(${hueOf(appId)} 45% 70% / .35)`; }
export function generatedIconSvg(appId, title) {
  const h = hashOf(appId);
  const hue = h % 360;
  const hue2 = (hue + 24 + ((h >>> 9) % 36)) % 360;
  const letter = ((String(title || appId).trim().match(/[\p{L}\p{N}]/u) || ['•'])[0]).toUpperCase()
    .replace(/[<>&"]/g, '');
  return `<svg xmlns="${SVG_NS}" viewBox="0 0 64 64" width="64" height="64"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">`
    + `<stop offset="0" stop-color="hsl(${hue} 64% 56%)"/><stop offset="1" stop-color="hsl(${hue2} 58% 36%)"/></linearGradient></defs>`
    + `<rect width="64" height="64" fill="url(#g)"/><text x="32" y="43" font-family="system-ui,-apple-system,'Segoe UI',sans-serif" `
    + `font-size="30" font-weight="600" fill="#ffffff" text-anchor="middle">${letter}</text></svg>`;
}

const MESSAGES = {
  de: {
    search: 'Apps suchen', pin: 'An Taskleiste heften', unpin: 'Von Taskleiste lösen', 'shell.home': 'Desktop',
    taskbar: 'Taskleiste', windowLayout: 'Fensteranordnung', windowClose: 'Schließen', windowFree: 'Freie Größe',
    windowMaximize: 'Maximieren', windowMinimize: 'Minimieren', windowSnapLeft: 'Linke Hälfte',
    windowSnapRight: 'Rechte Hälfte', windowSnapTop: 'Obere Hälfte', windowSnapBottom: 'Untere Hälfte',
    windowSnapTopLeft: 'Oben links anordnen', windowSnapTopRight: 'Oben rechts anordnen',
    windowSnapBottomLeft: 'Unten links anordnen', windowSnapBottomRight: 'Unten rechts anordnen',
    windowFullscreen: 'Vollbild', windowFullscreenExit: 'Vollbild beenden',
    newApp: 'Neue App', source: 'Quelltext',
  },
  en: {
    search: 'Search apps', pin: 'Pin to taskbar', unpin: 'Unpin from taskbar', 'shell.home': 'Desktop',
    taskbar: 'Taskbar', windowLayout: 'Window layout', windowClose: 'Close', windowFree: 'Free size',
    windowMaximize: 'Maximize', windowMinimize: 'Minimize', windowSnapLeft: 'Left half',
    windowSnapRight: 'Right half', windowSnapTop: 'Top half', windowSnapBottom: 'Bottom half',
    windowSnapTopLeft: 'Top left quarter', windowSnapTopRight: 'Top right quarter',
    windowSnapBottomLeft: 'Bottom left quarter', windowSnapBottomRight: 'Bottom right quarter',
    windowFullscreen: 'Full screen', windowFullscreenExit: 'Exit full screen',
    newApp: 'New app', source: 'Source',
  },
};

let current = null;

// M-02: Überlagerungen des Editors, die Esc selbst schließt (Monaco: Vorschläge, Suchen/Ersetzen, Hover, Parameterhilfe,
// Umbenennen, Peek, Kontextmenü). Ist eine offen, gehört das erste Esc dem Editor; erst das nächste führt zur App.
const EDITOR_OVERLAY_SEL = '.suggest-widget.visible, .find-widget.visible, .parameter-hints-widget.visible, '
  + '.monaco-hover:not(.hidden), .rename-box, .zone-widget, .context-view, .monaco-menu-container';
function editorOverlayOpen(editorEl) {
  const shown = (o) => { const r = o?.getBoundingClientRect(); return !!r && r.width > 0 && r.height > 0 && (o.checkVisibility?.() ?? true); };
  for (const o of editorEl.querySelectorAll(EDITOR_OVERLAY_SEL)) if (shown(o)) return true;
  // Kontextmenü: Monaco rendert es in einen Shadow-Root (.shadow-root-host, Höhe 0) im Editor.
  for (const h of editorEl.querySelectorAll('.shadow-root-host')) if (shown(h.shadowRoot?.querySelector('.monaco-menu'))) return true;
  return false;
}

function createBus() {
  const handlers = new Map();
  return {
    on(name, fn) { if (!handlers.has(name)) handlers.set(name, new Set()); handlers.get(name).add(fn); return { id: name }; },
    off(name, fn) { handlers.get(name)?.delete(fn); },
    emit(name, payload) { for (const fn of handlers.get(name) || []) { try { fn(payload); } catch (e) { console.error(e); } } },
  };
}

function parsePx(value) {
  const n = Number.parseFloat(String(value || ''));
  return Number.isFinite(n) ? Math.round(n) : null;
}

function cssLinkFor(href) {
  return new Promise((resolve) => {
    const link = document.createElement('link');
    link.rel = 'stylesheet';
    link.href = href;
    link.dataset.v4FrameCss = '';
    link.addEventListener('load', () => resolve(link), { once: true });
    link.addEventListener('error', () => resolve(link), { once: true });
    document.head.appendChild(link);
  });
}

// Der Loader uebergibt laut §16.15 die Shell-Kernel-API; der Kern-Stub (sim.startShell/swapShell)
// uebergibt den StubKernel, der sie unter .shell traegt. Beides wird angenommen.
export function resolveShellKernel(kernel) {
  if (kernel?.apps?.subscribe) return kernel;
  const shell = kernel?.shell || (typeof kernel?.shellApi === 'function' ? kernel.shellApi() : null);
  return shell?.apps?.subscribe ? shell : null;
}

export async function startShell(loaderKernel, options = {}) {
  if (current) await stopShell();
  const shellKernel = resolveShellKernel(loaderKernel);
  if (!shellKernel) throw new Error('startShell: kernel.apps (LiveQuery) required');
  const kernel = adaptKernel(loaderKernel);
  const shellVer = (/\/shell@([^/~]+)[/~]/.exec(import.meta.url) || [])[1] || 'dev';
  // Artefakt-URL baut der Kernel (§16.39 Nr. 1, K-1d): ohne Bindung mit der aktiven Shell, merkt sich nichts. Die
  // Shell gibt keine Shell-Version mit; App-Code eines Fensters lädt über die Bindung seines Mounts (app-host.js).
  const artifactUrl = (appId, ver, path) => {
    try { return kernel.artifacts.url(String(appId), String(ver), String(path || '').replace(/^\.?\/+/, '')); } catch { return null; }
  };
  const prefs = kernel.preferences(shellVer);
  // Ohne Snapshot vom Loader (Neuladen desselben Tabs): Fensteranordnung aus kernel.windowState.
  let restoredIn = options.restored;
  if (restoredIn == null && options.restoreFromWindowState !== false) {
    try { restoredIn = await kernel.windowState.get(SHELL_STATE_KEY); } catch { restoredIn = null; }
  }
  const restored = restoredIn && Number(restoredIn.version) === FRAME_SNAPSHOT_VERSION ? restoredIn : null;
  const createWindowHost = options.createWindowHost || createAppWindowHost;
  const onEvent = typeof options.onEvent === 'function' ? options.onEvent : null;
  const mountPoint = options.root || document.getElementById('ctox-shell') || document.body;
  const lang = String(kernel.locale || document.documentElement.lang || navigator.language || 'de').slice(0, 2);
  const dict = MESSAGES[lang] || MESSAGES.de;
  const t = (key, fallback) => dict[key] || fallback || key;
  const startedAt = performance.now();

  // Neues CSS laden; das CSS der alten Shell bleibt bis zum Umschalten (der alte Rahmen steht noch).
  // Standardschnittstelle Theme (FR2 Zusatz 1): Theme-Tokens (nur die von Apps genutzten, theme/theme.css), die
  // gemeinsame base.css des Shell-Artefakts (versioniert über die URL dieses Moduls: /~a/shell@<v>/shared/base.css)
  // und das Rahmen-CSS. Reihenfolge = Kaskade.
  const cssLinks = await Promise.all([
    new URL('./theme/theme.css', import.meta.url).href,
    new URL('../../shared/base.css', import.meta.url).href,
    // FR6a: Shell-V2-Fensterchrome wörtlich aus app.css (Icon-Header, Kantenverlauf, Eckgriffe, Hover-Icons).
    new URL('../../shared/shell-v2-chrome.css', import.meta.url).href,
    new URL('./frame.css', import.meta.url).href,
    // TOPBAR: Shell-V2-Kopfleiste wörtlich aus app.css (lab/extract-topbar-css.mjs); Teil des atomar getauschten Satzes.
    // Nach frame.css, damit dessen Rahmen-Grundregeln (.v4f :where(button) { color: inherit }) die Kopfleiste nicht treffen.
    new URL('../../shared/shell-v2-topbar.css', import.meta.url).href,
    // FR6d: Startmenü, Desktop und Crew-Leiste der Shell-V2 wörtlich (lab/extract-desktop-css.mjs), Hülle [data-v4-v2].
    new URL('./shell-v2-desktop.css', import.meta.url).href,
  ].map(cssLinkFor));
  const cssLink = cssLinks[cssLinks.length - 1];

  // Reste eines abgebrochenen Starts weg; ein abtretender Rahmen (Doppelpuffer) bleibt bis zum Umschalten.
  for (const stale of mountPoint.querySelectorAll(':scope > [data-v4-frame]:not([data-v4-retiring])')) stale.remove();
  const retiring = [...mountPoint.querySelectorAll(':scope > [data-v4-frame][data-v4-retiring]')];
  const root = document.createElement('div');
  root.className = 'v4f';
  root.dataset.v4Frame = '';
  root.dataset.shellVer = shellVer;
  if (retiring.length) {
    // Unsichtbar aufbauen: opacity 0 behält Layout, Fokus und Scrollpositionen (anders als visibility: hidden).
    root.dataset.v4Building = '';
    root.style.opacity = '0';
    root.style.pointerEvents = 'none';
  }
  root.innerHTML = '<main class="v4f-desk" data-v4-desk><div class="v4f-layer" data-v4-layer></div><div class="v4f-snap" data-v4-snap hidden></div></main>';
  mountPoint.appendChild(root);
  const desk = root.querySelector('[data-v4-desk]');
  const layer = root.querySelector('[data-v4-layer]');

  // Hell/Dunkel: data-theme am Dokument ist die Schnittstelle (theme/theme.css, base.css, frame.css). Hat der Host
  // es nicht gesetzt, folgt der Rahmen prefers-color-scheme und hält es aktuell.
  const html = document.documentElement;
  const media = typeof matchMedia === 'function' ? matchMedia('(prefers-color-scheme: dark)') : null;
  const themeOwned = !html.dataset.theme || html.dataset.v4ThemeAuto === '1';
  const applyTheme = () => { if (themeOwned) { html.dataset.theme = media?.matches ? 'dark' : 'light'; html.dataset.v4ThemeAuto = '1'; } };
  applyTheme();
  media?.addEventListener?.('change', applyTheme);

  const env = createHostEnvironment(kernel);
  const bus = createBus();
  const geometry = new Map(); // ownerId -> zuletzt gespeicherte Geometrie (window-manager persistence)
  const pendingGeometry = new Map(); // ownerId -> Geometrie fuer create() beim Wiederherstellen
  const wm = createWindowManager({
    windowLayer: layer,
    surfaceEl: desk,
    rootEl: root,
    snapPreviewEl: root.querySelector('[data-v4-snap]'),
    eventBus: bus,
    t,
    persistence: {
      load: (ownerId) => pendingGeometry.get(ownerId) || null,
      save: (ownerId, snap) => geometry.set(ownerId, snap),
    },
  });

  const state = {
    kernel,
    root,
    wm,
    bus,
    apps: [],
    appById: new Map(),
    windows: new Map(), // winId -> record
    pins: [],
    pinsUpdatedAtMs: 0,
    launcherOrder: [],
    prefs,
    seq: 0,
    unsubs: [],
    manifests: new Map(),
    stopped: false,
    t,
    geometry,
    env,
    cssLink,
    cssLinks,
    brokenVer: new Map(), // appId -> {failed, good}: neue Fenster starten nicht auf einer gerade zurückgefallenen Version
  };
  current = state;
  state.unsubs.push(() => env.destroy(), () => wm.dispose(), () => media?.removeEventListener?.('change', applyTheme));
  // E24/WJB-0, WJB-D: Workjet-Brücken (globalThis.workjet*) über kernel.command/kernel.query; ein neuer Start ersetzt die
  // alten. Die Mobil-Hostbrücke bekommt den Rahmengriff und den Mobil-Katalog aus kernel.apps (layout.presentation.mobile,
  // K-WJ-D1); der Katalog wartet auf die erste App-Liste.
  state.appsLoaded = new Promise((resolve) => { state.resolveAppsLoaded = resolve; });
  state.appsListeners = new Set();
  import('../shell/workjet/index.js').then((m) => m.installWorkjetBridges({
    kernel, shellVer, shell: () => shellHandle(),
    mobileCatalog: () => state.appsLoaded.then(() => mobileCatalogOf(state.apps)),
    mobileCatalogChanges: (cb) => { state.appsListeners.add(cb); return () => state.appsListeners.delete(cb); },
  })).catch((e) => console.warn('[v4 frame] workjet bridges', e));
  // ZONE45 (§16.45): ehrlicher Verbindungszustand. Badge je App-Fenster (attachSyncState in openApp), Zustandspunkt der
  // Kopfleiste über currentSyncState() aus ../shell/sync-state.js. App-Collections = collections des Manifests.
  const syncCols = new Map(); // `${appId}@${ver}` → Collections aus dem Manifest (null = wird geladen)
  state.sync = createSyncState({
    kernel,
    appCollections(appId) {
      const ver = state.appById.get(appId)?.ver;
      if (!ver) return [];
      const key = `${appId}@${ver}`;
      if (!syncCols.has(key)) {
        syncCols.set(key, null);
        Promise.resolve(kernel.artifacts?.manifest?.(appId, ver)).then((m) => Object.keys(m?.collections || {}), () => [])
          .then((names) => { syncCols.set(key, names); if (!state.stopped) state.sync.refresh(); });
      }
      return syncCols.get(key) || [];
    },
  });
  state.unsubs.push(() => state.sync.destroy());

  // ---------- Icons und Layout aus Manifest ----------
  // §16.19 Nr. 6: layout kommt mit kernel.apps; nur ältere Kernel ohne layout im Eintrag brauchen das Manifest.
  function svgDataUrl(svg) {
    const text = String(svg || '').trim();
    return /^(<\?xml[^>]*>\s*)?<svg\b/i.test(text) ? `data:image/svg+xml;charset=utf-8,${encodeURIComponent(ensureSvgXmlns(text))}` : '';
  }
  // icon_asset *.svg, das nicht dekodiert: Bytes über das Artefakt lesen (Kernel; sonst dieselbe URL wie das Bild),
  // xmlns ergänzen. Ergebnis je URL einmal (null = nicht reparierbar → Monogramm).
  const repairedIcons = new Map(); // url -> Promise<string|null>
  function repairSvgIcon(app) {
    const url = app.iconAsset;
    if (repairedIcons.has(url)) return repairedIcons.get(url);
    const path = String(app.layout?.icon_asset || '');
    const job = (async () => {
      let text = '';
      try {
        const m = kernel.artifacts?.read && app.ver ? await kernel.artifacts.manifest(app.id, app.ver) : null;
        const hash = m?.files?.[path];
        if (hash) {
          const blob = await kernel.artifacts.read(hash);
          text = typeof blob === 'string' ? blob : await blob?.text?.() ?? '';
        }
      } catch { text = ''; }
      if (!text) {
        try { const r = await fetch(url); text = r.ok ? await r.text() : ''; } catch { text = ''; }
      }
      const fixed = ensureSvgXmlns(text);
      return fixed !== text ? svgDataUrl(fixed) : null;
    })();
    repairedIcons.set(url, job);
    return job;
  }
  function appView(entry) {
    const ver = String(entry.activated_ver || '');
    const manifest = state.manifests.get(`${entry.id}@${ver}`);
    const layout = entry.layout_known ? entry.layout : (manifest?.layout || entry.layout || {});
    return {
      id: String(entry.id),
      // Sichtbare Begriffe über das Glossar (englische Rohwerte aus Manifesten → deutsch, Unbekanntes unverändert).
      title: appTitle(String(entry.title || entry.id), lang),
      ver,
      category: categoryLabel(String(layout.category || ''), lang),
      iconAsset: layout.icon_asset && ver ? artifactUrl(entry.id, ver, layout.icon_asset) : svgDataUrl(layout.icon_svg),
      // MAN3: layout.icon_asset_srcset („assets/icon.jpg 1x, assets/icon@512.jpg 2x“), jeder Pfad über das Artefakt.
      iconSrcSet: layout.icon_asset && ver ? iconSrcSetOf(entry.id, ver, layout.icon_asset_srcset) : '',
      layout,
      styles: Array.isArray(entry.styles) && entry.styles.length ? entry.styles : (manifest?.styles || []),
      entry: entry.entry || manifest?.entry || 'index.js',
      raw: entry,
    };
  }

  const manifestLoads = new Map(); // key -> Promise
  function loadManifest(entry, { rerender = true } = {}) {
    const ver = String(entry.activated_ver || '');
    const key = `${entry.id}@${ver}`;
    if (entry.layout_known || !kernel.artifacts?.manifest || !ver) return Promise.resolve();
    if (manifestLoads.has(key)) return manifestLoads.get(key);
    state.manifests.set(key, null);
    const load = Promise.resolve(kernel.artifacts.manifest(entry.id, ver)).then((manifest) => {
      state.manifests.set(key, manifest || {});
      if (!state.stopped && rerender && state.rendered) applyApps(state.rawApps || []);
    }).catch(() => state.manifests.set(key, {}));
    manifestLoads.set(key, load);
    return load;
  }

  // ---------- Icons: EINE Quelle für Launcher, Fenster, Taskleiste, Code-Ansicht (FR6a, M-09, M-17) ----------
  // resolveIcon(app) → Promise<{url, source}> je App-Version einmal: icon_svg (data-URL, xmlns ergänzt) bzw. icon_asset
  // über artifacts.url; dekodiert ein *.svg nicht (oder ohne Breite und ohne xmlns), wird es repariert. Ohne Icon oder
  // unreparierbar: einmal ein generiertes Icon (Verlauf aus app_id-Hash + Anfangsbuchstabe, source 'generated').
  // Alle Stellen zeigen dieselbe URL; bis sie geladen ist, steht ein gleich großer Platzhalter in der App-Farbe.
  const resolvedIcons = new Map(); // `${id}@${ver}|${asset}` -> Promise<{url, source}>
  const probeImage = (url) => new Promise((resolve) => {
    const img = new Image();
    img.decoding = 'async';
    img.onload = () => resolve({ ok: true, w: img.naturalWidth });
    img.onerror = () => resolve({ ok: false, w: 0 });
    img.src = url;
  });
  function iconSrcSetOf(id, ver, srcset) {
    if (typeof srcset !== 'string' || !srcset.trim()) return '';
    return srcset.split(',').map((part) => part.trim().split(/\s+/)).filter((p) => p[0])
      .map(([path, desc]) => `${artifactUrl(id, ver, path)}${desc ? ` ${desc}` : ''}`).join(', ');
  }
  // srcset nur, wenn das Bild das Manifest-Icon selbst ist (nicht repariert, nicht generiert).
  const srcSetFor = (app, url) => (app.iconSrcSet && url === app.iconAsset ? app.iconSrcSet : '');
  function resolveIcon(app) {
    const key = `${app.id}@${app.ver}|${app.iconAsset || ''}`;
    if (resolvedIcons.has(key)) return resolvedIcons.get(key);
    const generated = () => ({ url: svgDataUrl(generatedIconSvg(app.id, app.title)), source: 'generated' });
    const job = (async () => {
      if (!app.iconAsset) return generated();
      const probe = await probeImage(app.iconAsset);
      const svgAsset = /\.svg$/i.test(String(app.layout?.icon_asset || '')) && !String(app.iconAsset).startsWith('data:');
      if (probe.ok && (probe.w > 0 || !svgAsset)) return { url: app.iconAsset, source: 'icon' };
      if (svgAsset) {
        const fixed = await repairSvgIcon(app);
        if (fixed) return { url: fixed, source: 'icon' };
        if (probe.ok) return { url: app.iconAsset, source: 'icon' };
      }
      return generated();
    })();
    resolvedIcons.set(key, job);
    return job;
  }
  function iconFor(app, slot) {
    if (!slot) return;
    const sig = `${app.id}@${app.ver}|${app.iconAsset || ''}|${app.iconSrcSet || ''}`;
    if (slot.dataset.v4IconSig === sig) return;
    slot.dataset.v4IconSig = sig;
    slot.classList.remove('is-glyph');
    slot.style.background = placeholderColor(app.id);
    const img = document.createElement('img');
    img.alt = '';
    img.draggable = false;
    img.decoding = 'async';
    img.style.visibility = 'hidden';
    img.addEventListener('load', () => { if (slot.dataset.v4IconSig === sig) { img.style.visibility = ''; slot.style.background = ''; } });
    slot.replaceChildren(img);
    resolveIcon(app).then(({ url, source }) => {
      if (slot.dataset.v4IconSig !== sig) return;
      slot.dataset.v4IconSource = source;
      if (source === 'icon' && url !== app.iconAsset) img.dataset.v4IconRepaired = '';
      const srcset = srcSetFor(app, url);
      if ((img.getAttribute('srcset') || '') !== srcset) { if (srcset) img.srcset = srcset; else img.removeAttribute('srcset'); }
      if (img.getAttribute('src') !== url) img.src = url;
    });
  }

  // ---------- Pins und Launcher-Reihenfolge (shell_preferences, §16.19 Nr. 3) ----------
  // Zeile {id: actor.id, taskbar_pins: {version, pins, updated_at_ms}, launcher_order: [app_id…], updated_at_ms}.
  // Der Shell-Snapshot trägt dieselben Werte; es gilt der jüngere Stand (resolveTaskbarPinState).
  const restoredPins = restored ? decodeTaskbarPinCache(restored.pins || null) : { pins: [], updatedAtMs: 0, present: false };
  let prefRow = null;
  try { prefRow = prefs ? ((await prefs.load?.()) ?? prefs.get()) : null; } catch { prefRow = null; }
  const remotePinState = decodeTaskbarPinCache(prefRow?.taskbar_pins ?? null);
  const pinState = resolveTaskbarPinState({
    localPins: restoredPins.pins,
    localUpdatedAtMs: restoredPins.updatedAtMs,
    localPresent: restoredPins.present,
    remotePins: remotePinState.present ? remotePinState.pins : null,
    remoteUpdatedAtMs: remotePinState.updatedAtMs,
  });
  state.pins = pinState.pins;
  state.pinsUpdatedAtMs = pinState.updatedAtMs;
  const orderOf = (row) => (Array.isArray(row?.launcher_order) ? row.launcher_order.map(String) : null);
  state.launcherOrder = (Number(prefRow?.updated_at_ms || 0) >= Number(restored?.prefs_updated_at_ms || 0) ? orderOf(prefRow) : null)
    ?? (Array.isArray(restored?.launcher_order) ? restored.launcher_order.map(String) : []);
  state.prefsUpdatedAtMs = Math.max(Number(prefRow?.updated_at_ms || 0), Number(restored?.prefs_updated_at_ms || 0));

  function savePreferences() {
    const now = Date.now();
    state.prefsUpdatedAtMs = now;
    if (!prefs) return;
    const row = {
      taskbar_pins: JSON.parse(encodeTaskbarPinCache(state.pins, state.pinsUpdatedAtMs)),
      launcher_order: state.launcherOrder.slice(),
      updated_at_ms: now,
    };
    state.prefsWrite = Promise.resolve().then(() => prefs.put(row)).catch((e) => console.warn('[v4 frame] shell_preferences', e));
  }
  function savePins() {
    state.pinsUpdatedAtMs = Date.now();
    savePreferences();
  }
  function setLauncherOrder(ids) {
    state.launcherOrder = [...new Set((ids || []).map(String))];
    savePreferences();
    launcher.setOrder(state.launcherOrder);
    state.desktop?.setOrder(state.launcherOrder);
  }
  // Live: eine andere Shell (anderer Tab, anderes Gerät) ändert die Zeile → übernehmen, wenn sie jünger ist.
  function applyPreferences(row) {
    if (state.stopped || !row) return;
    const d = decodeTaskbarPinCache(row.taskbar_pins ?? null);
    let changed = false;
    if (d.present && d.updatedAtMs > state.pinsUpdatedAtMs) {
      state.pins = d.pins;
      state.pinsUpdatedAtMs = d.updatedAtMs;
      changed = true;
    }
    const order = orderOf(row);
    if (order && Number(row.updated_at_ms || 0) > state.prefsUpdatedAtMs && JSON.stringify(order) !== JSON.stringify(state.launcherOrder)) {
      state.launcherOrder = order;
      launcher.setOrder(order);
      state.desktop?.setOrder(order);
      changed = true;
    }
    state.prefsUpdatedAtMs = Math.max(state.prefsUpdatedAtMs, Number(row.updated_at_ms || 0));
    if (changed && state.rendered) { launcher.refresh(); renderTaskbar(); }
  }

  // Pin per Ziehen setzen bzw. umordnen: vor beforeId, sonst ans Ende.
  function dropPin(appId, beforeId = null) {
    if (!state.appById.has(appId)) return;
    const next = state.pins.filter((id) => id !== appId);
    const at = beforeId ? next.indexOf(beforeId) : -1;
    next.splice(at < 0 ? next.length : at, 0, appId);
    if (JSON.stringify(next) === JSON.stringify(state.pins)) return;
    state.pins = next;
    savePins();
    launcher.refresh();
    renderTaskbar();
  }

  function togglePin(appId) {
    state.pins = state.pins.includes(appId) ? state.pins.filter((id) => id !== appId) : [...state.pins, appId];
    savePins();
    launcher.refresh();
    renderTaskbar();
  }

  // ---------- Fenster ----------
  function windowList() {
    const described = new Map(wm.listWindows().map((w) => [w.id, w]));
    return [...state.windows.values()].map((rec) => {
      const d = described.get(rec.id) || {};
      // FR6b: Taskleisten-Gruppe aus layout.presentation.taskbar_group (z. B. „crew“ → ein Eintrag „Crew“), sonst je App.
      const tg = taskGroupOf(state.appById.get(rec.appId));
      const group = tg?.key ?? null;
      const groupTitle = tg?.title ?? null;
      return { id: rec.id, appId: rec.appId, title: rec.title, state: d.state || 'normal', focused: !!d.isFocused, z: Number(rec.handle.element.style.zIndex) || 0, group, groupTitle };
    });
  }

  function renderTaskbar() {
    if (state.stopped) return;
    // FR6c: einzige Leiste = Reiterleiste der Kopfleiste (topbar.js), gruppiert wie FR6b.
    const list = windowList();
    state.topbar?.render({ pins: state.pins, apps: state.apps, windows: list });
    const front = list.filter((w) => w.state !== 'minimized').sort((a, b) => b.z - a.z)[0];
    state.desktop?.setRunning(new Set(list.map((w) => w.appId)), front?.appId ?? null);
  }

  function windowShellApi(rec) {
    return {
      setTitle(title) {
        rec.title = appTitle(String(title || rec.appTitle), lang);
        rec.handle.setTitle(rec.title);
        renderTaskbar();
      },
      headerSlot: rec.handle.element.querySelector('[data-window-actions]'),
      // Eigene Dateien über die Bindung des laufenden Mounts (gleiche Wurzel wie der geladene Code), sonst Kernel-URL.
      assetUrl: (path) => rec.host?.assetUrl?.(path) ?? artifactUrl(rec.appId, rec.host?.ver || rec.ver, path),
      navigate: (appId, params) => navigateTo(appId, params, rec.appId),
      // api.shell.openSource(appId?): Quelltext-Ansicht der App (Standard: die eigene). false → Zone (app-host.js).
      // options {tab, reportId} (geprüft in app-host.js sourceFocusOf) → Reiter/Meldung der Code-Ansicht (Strang LOOP).
      openSource: (appId, focus) => !!openSourceView(appId ?? rec.appId, { focus }),
      close: () => wm.destroy(rec.id),
      // Blatt (api.shell.dialog) holt sein Fenster nach vorn.
      focus: () => { if (!state.stopped && state.windows.has(rec.id)) { wm.focus(rec.id); renderTaskbar(); } },
      window: {
        close: () => wm.destroy(rec.id),
        // Vollbild als Fensterfunktion der Shell (Bedienkonzept §Vollbild, M-11): Element-Vollbild des Fensterinhalts.
        fullscreen: () => enterFullscreen(rec),
        exitFullscreen: () => exitFullscreen(rec),
        get isFullscreen() { return isFullscreen(rec); },
        get mode() { return wm.describe(rec.id)?.state || 'normal'; },
        get element() { return rec.handle.container; },
      },
    };
  }

  // api.shell.navigate(appId, params): generische Standardschnittstelle ohne App-Sonderfälle. Die Parameter gehen
  // unverändert (als Kopie, damit Fenster sich nichts teilen) als RouteEvent {from, to, params} an onRoute der
  // Ziel-App (z. B. desktop: navigate('ctox', {chat})). Ziel offen → fokussieren + route, sonst neues Fenster,
  // route nach mount. Unbekanntes Ziel → false (der Host meldet es in die Zone).
  function navigateTo(appId, params, from = 'shell') {
    if (!state.appById.get(appId)?.ver) return false;
    let p = params ?? null;
    if (p) { try { p = structuredClone(p); } catch { p = { ...p }; } }
    const target = [...state.windows.values()].filter((r) => r.appId === appId)
      .sort((a, b) => Number(b.handle.element.style.zIndex) - Number(a.handle.element.style.zIndex))[0];
    if (target && p) { wm.focus(target.id); target.host?.route?.(p, { from }); renderTaskbar(); return true; }
    return !!openApp(appId, { params: p, from });
  }

  function openApp(appId, { anchor = null, restore = null, params = null, from = 'shell', geometry: g = null, multi = false } = {}) {
    const app = state.appById.get(appId);
    if (!app || !app.ver) return null;
    // Ein Fenster je App ist der Normalfall: vorhandenes Fenster nach vorn holen. Ein zweites Fenster (multi) nur,
    // wenn das Layout es nicht ausschließt (presentation.multi_instance === false: Apps mit modulweitem Zustand,
    // Liste in INTEGRATION.md FR2-4).
    const single = app.layout?.presentation?.multi_instance === false;
    if (!restore && (!params || single) && (!multi || single)) {
      const existing = [...state.windows.values()].find((r) => r.appId === appId);
      if (existing) {
        wm.focus(existing.id);
        if (params) existing.host?.route?.(params, { from });
        renderTaskbar();
        return existing;
      }
    }
    const ownerId = restore?.ownerId || `${OWNER_PREFIX}${appId}#${++state.seq}`;
    const seqMatch = /#(\d+)$/.exec(ownerId);
    if (seqMatch) state.seq = Math.max(state.seq, Number(seqMatch[1]));
    if (restore) pendingGeometry.set(ownerId, restore);
    const anchorEl = anchor;
    const iconAnchor = launcherSizedAnchor(anchorEl);
    // Neue Fenster gestaffelt um die Mitte, damit sich Titelzeilen nicht verdecken.
    // Größen aus layout (§16.19 Nr. 6): default_width/height, min_width/min_height; sonst Rahmen-Standard.
    const vp = wm.getViewport();
    const L = app.layout || {};
    let iconSource = 'icon';
    const num = (v) => (Number.isFinite(Number(v)) && Number(v) > 0 ? Number(v) : null);
    const width = g?.w ?? Math.min(num(L.default_width) ?? 760, Math.max(320, vp.w - vp.left - vp.right - 48));
    const height = g?.h ?? Math.min(num(L.default_height) ?? 520, Math.max(200, vp.h - vp.top - vp.bottom - 48));
    const step = (state.windows.size % 6) * 28;
    const x = g?.x ?? Math.max(vp.left, Math.round(vp.left + (vp.w - vp.left - vp.right - width) / 2 - 70 + step));
    const y = g?.y ?? Math.max(vp.top, Math.round(vp.top + (vp.h - vp.top - vp.bottom - height) / 2 - 50 + step));
    const handle = wm.create({
      ownerId,
      title: restore?.title || app.title,
      // Dieselbe aufgelöste Icon-URL wie Launcher/Taskleiste (Promise); Kantenpalette aus deren Pixeln (M-17).
      iconAsset: resolveIcon(app).then((r) => { iconSource = r.source; return r.url; }),
      iconSource: () => iconSource,
      iconSrcSet: (url) => srcSetFor(app, url),
      width,
      height,
      x,
      y,
      minWidth: num(L.min_width) ?? undefined,
      minHeight: num(L.min_height) ?? undefined,
      shellGeometryContract: 'v4-frame',
      iconAnchorRect: iconAnchor,
    });
    pendingGeometry.delete(ownerId);
    // Chrome-Attribut statt data-app: [data-app="x"] gehört allein dem Inhalts-root der App (App-CSS ist darauf
    // gescoped und darf Fensterknöpfe nicht treffen).
    handle.element.dataset.windowApp = appId;
    handle.element.dataset.windowId = ownerId;
    // layout.frame_palette wird nicht ausgewertet (M-17): die Kanten kommen allein aus den Icon-Pixeln.
    handle.container.classList.add('v4f-content');
    // Zonen-Platz: Teil des Chromes, unter dem Inhaltsbereich. Ohne Einträge 0 px (status-zone.js).
    const zoneSlot = document.createElement('div');
    zoneSlot.className = 'v4f-zone-slot';
    zoneSlot.dataset.v4ZoneSlot = '';
    handle.container.after(zoneSlot);
    const broken = state.brokenVer.get(appId);
    const ver = broken && broken.failed === app.ver ? broken.good : app.ver;
    const rec = {
      id: handle.id,
      ownerId,
      appId,
      ver,
      appTitle: app.title,
      title: restore?.title || app.title,
      handle,
      host: null,
    };
    state.windows.set(rec.id, rec);
    attachChrome(rec, app);
    const api = {
      kernel,
      loaderKernel,
      windowId: ownerId,
      windowEl: handle.element,
      zoneSlot,
      shell: windowShellApi(rec),
      shellStatus: env.status,
      onEvent,
      params,
      routeFrom: from,
      onRolledBack: ({ failedVer, ver: good }) => state.brokenVer.set(appId, { failed: failedVer, good }),
    };
    try {
      rec.host = createWindowHost({
        root: handle.container,
        appRecord: { id: appId, ver: rec.ver, entry: app.entry, title: app.title, styles: app.styles },
        api,
        restored: restore?.app_snapshot ?? null,
      });
    } catch (error) {
      console.error('[v4 frame] window host failed', appId, error);
      wm.destroy(rec.id);
      return null;
    }
    // ZONE45: Verbindungszustand der App-Collections am Meldungs-Badge (räumt sich mit der Zone bzw. dem Sync-Zustand).
    if (rec.host?.zone) attachSyncState(rec.host.zone, state.sync, { appId });
    // Strang LOOP (§3a Schritt 5): „Deine Meldung #N ist behoben“ einmal am Meldungs-Badge dieses Fensters (räumt sich mit der Zone).
    if (rec.host?.zone) { try { watchFixedReports({ api: kernel.appApi('shell', shellVer), appId, zone: rec.host.zone, sheet: rec.host.win?.sheet ?? null }); } catch { /* ohne Shell-App-API keine Rückmeldung */ } }
    renderTaskbar();
    return rec;
  }

  // ---------- Fenster-Chrome je App-Fenster (FR6a) ----------
  // Das Chrome bleibt exakt Shell-V2 (Icon-Header oben links, oben rechts nur Layout und Schließen). Der Rahmen misst
  // die Breite des Knopfclusters und stellt sie als --shell-controls-inset bereit (reservierte Zone, frame.css).
  // Code-Ansicht: Klick auf den Icon-Header öffnet das App-Menü (mit den Meldungen) mit „Code ansehen“ bzw. „Zurück zur
  // App“ (nur mit apps.source.view); sie wechselt IM SELBEN Fenster, der App-Inhalt bleibt montiert, die Code-Ansicht
  // bleibt beim Zurückschalten offen (Entwurf und Cursor bleiben). Esc führt zurück, Strg/Cmd+Umschalt+E schaltet um.
  // Doppelklick auf eine freie Stelle eines Pane-Kopfs maximiert bzw. stellt wieder her.
  function attachChrome(rec) {
    const el = rec.handle.element;
    const controls = el.querySelector('[data-window-controls]');
    const content = rec.handle.container;
    el.dataset.v4Mode = 'app';
    rec.code = { host: null, view: null, opening: null, closed: false };
    // Shell-Tausch: die Code-Ansicht des alten Rahmens schließt (Entwurf liegt im Kern); der neue Rahmen öffnet sie neu.
    state.unsubs.push(() => { rec.code.closed = true; try { rec.code.view?.close?.(); } catch { /* still */ } rec.code.view = null; });
    const codeAllowed = () => canViewSource() && !!sourceKernel;
    // FR6b (M-06): „Problem melden“ und „Wunsch äußern“ für ALLE Nutzer; „Code ansehen“ bleibt an apps.source.view.
    const reportItems = () => [
      { id: 'report-bug', label: 'Problem melden', hint: 'Strg+Alt+B', run: () => reportProblem({ appId: rec.appId, kind: 'bug', windowId: rec.ownerId }) },
      { id: 'report-wish', label: 'Wunsch äußern', run: () => reportProblem({ appId: rec.appId, kind: 'wish', windowId: rec.ownerId }) },
    ];
    APP_MENU.set(el, () => [...(codeAllowed()
      ? [el.dataset.v4Mode === 'code'
        ? { id: 'app', label: 'Zurück zur App', hint: 'Esc', run: () => showApp(rec) }
        : { id: 'code', label: 'Code ansehen', hint: 'Strg+Umschalt+E', run: () => showCode(rec) }]
      : []), ...reportItems()]);
    const syncInset = () => {
      const w = controls.getBoundingClientRect().width;
      if (w > 0) el.style.setProperty('--shell-controls-inset', `${Math.ceil(w + 8)}px`);
    };
    if (typeof ResizeObserver === 'function') {
      rec.chromeRo = new ResizeObserver(syncInset);
      rec.chromeRo.observe(controls);
      state.unsubs.push(() => rec.chromeRo?.disconnect()); // Shell-Tausch: der alte Rahmen baut ohne window:closing ab
    }
    // ZONE2 (K24, Befund T3-1): Zonen-Garantie für JEDE App, auch ohne Kit. Beobachtet den Inhaltsbereich (der App-Host
    // ersetzt die Wurzel bei jedem mount) und rückt die App-Wurzel aus, wenn ein Bedienelement eine Zone schneidet.
    rec.zoneGuard = createZoneGuard({ pick: (c) => c.querySelector(':scope > .v4-app-root') });
    rec.zoneGuard.attach(content);
    state.unsubs.push(() => rec.zoneGuard?.detach());
    const escBackAllowed = () => el.dataset.v4Mode === 'code'
      && el.querySelector('[data-window-layout-menu]')?.hidden !== false && !el.querySelector('.v4-status-zone__list:not([hidden])');
    // M-02: Monaco verbraucht Esc immer (preventDefault, bei Vorschlägen und Suchen zusätzlich stopPropagation), auch
    // ohne offene Überlagerung. Darum entscheidet der Rahmen für den Editor-Eingang in der Capture-Phase, BEVOR Monaco
    // das Ereignis sieht: keine Überlagerung offen → zurück zur App (Monaco bekommt das Esc nicht, Auswahl und Cursor
    // bleiben); sonst nichts tun, Monaco schließt die Überlagerung, das nächste Esc führt zurück.
    el.addEventListener('keydown', (ev) => {
      if (ev.key !== 'Escape' || ev.defaultPrevented || !escBackAllowed()) return;
      const tg = ev.target;
      const editorEl = tg?.closest?.('.v4-src-ed--monaco');
      if (!editorEl || !rec.code?.host?.contains(editorEl) || tg.shadowRoot || !tg.matches('textarea.inputarea, .native-edit-context')) return;
      if (editorOverlayOpen(editorEl)) return;
      ev.preventDefault();
      ev.stopPropagation();
      showApp(rec);
    }, true);
    // Übrige Ziele (textarea-Rückfall, Reiter, Dateibaum): Bubble-Phase; wer Esc selbst braucht (Agent-Eingabe mit Text,
    // Umbenennen, „…“-Menü, Blätter), verhindert den Standard bzw. stoppt die Weitergabe.
    el.addEventListener('keydown', (ev) => {
      if (ev.key === 'Escape' && !ev.defaultPrevented && escBackAllowed()) {
        ev.preventDefault();
        showApp(rec);
      }
    });
    // M-11: „Vollbild“ als letzter Eintrag des Layout-Menüs (Bedienkonzept: Vollbild über das Layout-Menü, F11,
    // Strg+Cmd+F). Gleiche Klassen und Attribute wie „Freie Größe“/„Minimieren“ (window-manager.js): Pfeiltasten (eine
    // Zeile unter „Minimieren“), Hover und Schließen nach Wahl übernimmt die Menüsteuerung des Fenstermanagers; die
    // Wirkung hängt hier. Beschriftung „Vollbild beenden“, solange das Fenster im Vollbild ist (fullscreenchange unten).
    const layoutMenu = el.querySelector('[data-window-layout-menu]');
    if (layoutMenu && !layoutMenu.querySelector('[data-window-layout-control="fullscreen"]')) {
      const row = 1 + Math.max(0, ...[...layoutMenu.querySelectorAll('[data-window-layout-control]')].map((b) => Number(b.dataset.row) || 0));
      const fsBtn = document.createElement('button');
      fsBtn.type = 'button';
      fsBtn.className = 'is-wide';
      fsBtn.dataset.windowLayoutControl = 'fullscreen';
      fsBtn.dataset.row = String(row);
      fsBtn.dataset.col = '1';
      fsBtn.style.gridRow = String(row);
      fsBtn.style.gridColumn = '1 / -1';
      fsBtn.tabIndex = -1;
      fsBtn.setAttribute('role', 'menuitem');
      const glyph = document.createElement('span');
      glyph.className = 'shell-window-layout-glyph shell-window-layout-glyph--fullscreen';
      glyph.setAttribute('aria-hidden', 'true');
      const label = document.createElement('span');
      label.className = 'shell-window-layout-label';
      fsBtn.append(glyph, label);
      rec.syncFullscreenItem = (on) => {
        const text = on ? t('windowFullscreenExit', 'Vollbild beenden') : t('windowFullscreen', 'Vollbild');
        label.textContent = text;
        fsBtn.title = text;
        fsBtn.setAttribute('aria-label', text);
        fsBtn.dataset.fullscreen = on ? 'on' : 'off';
      };
      rec.syncFullscreenItem(false);
      const toggle = () => { if (isFullscreen(rec)) exitFullscreen(rec); else enterFullscreen(rec); };
      // Klick: der Fenstermanager schließt danach das Menü (applyLayoutControl kennt „fullscreen“ nicht → nur schließen).
      fsBtn.addEventListener('click', toggle);
      // Enter/Leertaste: die Menüsteuerung verhindert den Standard (kein Klick), darum hier direkt.
      fsBtn.addEventListener('keydown', (ev) => { if (ev.key === 'Enter' || ev.key === ' ') toggle(); });
      layoutMenu.append(fsBtn);
    }
    content.addEventListener('dblclick', (ev) => {
      const head = ev.target.closest?.('.ctox-pane-header');
      if (!head || ev.target.closest('button, a, input, select, textarea, [role="button"], [tabindex], [contenteditable]')) return;
      ev.preventDefault();
      if (wm.describe(rec.id)?.state === 'maximized') rec.handle.restore(); else rec.handle.maximize();
    });
  }
  function setModePressed(rec, code) {
    rec.handle.element.dataset.v4Mode = code ? 'code' : 'app';
  }
  function showCode(rec, focus = null) {
    if (!canViewSource() || !sourceKernel || rec.view || !rec.code) return false;
    const app = state.appById.get(rec.appId);
    if (!app) return false;
    setModePressed(rec, true);
    if (rec.code.host) {
      rec.code.host.hidden = false;
      if (focus) (rec.code.view ? Promise.resolve() : rec.code.opening)?.then(() => rec.code.view?.focus?.(focus));
      return true;
    }
    const host = document.createElement('div');
    host.className = 'v4-src-host';
    host.dataset.v4SrcHost = '';
    // Neben dem Inhaltsbereich (den der App-Host besitzt und beim mount neu befüllt), im Fenster über dem Inhalt.
    rec.handle.container.after(host);
    rec.code.host = host;
    const zone = rec.host?.zone || null;
    rec.code.opening = Promise.all([loadSourceCss(), import('../source/source-mode.js')]).then(async ([, mod]) => {
      if (rec.code.closed || state.stopped || rec.code.host !== host) return;
      const opened = await mod.openSourceMode({ root: host, appRecord: { id: app.id, title: app.title, active_ver: app.raw.active_ver }, kernel: sourceKernel, api: { actor: { permissions: actorPerms } }, zone: zone || { report() {}, clear() {} }, sheet: rec.host?.win?.sheet ?? null, focus });
      if (rec.code.closed || state.stopped || rec.code.host !== host) opened?.close(); else rec.code.view = opened;
    }).catch((e) => zone?.report({ level: 'error', code: 'source.open', text: `Quelltext nicht ladbar: ${e?.message ?? e}`, source: 'shell' }));
    return true;
  }
  function showApp(rec) {
    setModePressed(rec, false);
    if (rec.code?.host) rec.code.host.hidden = true;
  }
  function closeCode(rec) {
    if (!rec.code) return;
    showApp(rec);
    rec.code.view?.close?.();
    rec.code.view = null;
    rec.code.host?.remove();
    rec.code.host = null;
  }

  // ---------- Vollbild (api.shell.window.fullscreen / exitFullscreen, F11, Strg+Cmd+F; Esc beendet) ----------
  const isFullscreen = (rec) => !!document.fullscreenElement && rec.handle.element.contains(document.fullscreenElement);
  async function enterFullscreen(rec) {
    if (isFullscreen(rec)) return true;
    try { await rec.handle.container.requestFullscreen?.({ navigationUI: 'hide' }); } catch { return false; }
    return isFullscreen(rec);
  }
  async function exitFullscreen(rec) {
    if (!isFullscreen(rec)) return true;
    try { await document.exitFullscreen(); } catch { return false; }
    return !document.fullscreenElement;
  }
  // M-11: Beschriftung des Layout-Menü-Eintrags folgt dem Zustand (auch bei Esc/F11 und Vollbild der App selbst).
  const onFullscreenChange = () => { for (const r of state.windows.values()) r.syncFullscreenItem?.(isFullscreen(r)); };
  document.addEventListener('fullscreenchange', onFullscreenChange);
  state.unsubs.push(() => document.removeEventListener('fullscreenchange', onFullscreenChange));
  const focusedRec = () => {
    const f = wm.listWindows().find((w) => w.isFocused && w.state !== 'minimized');
    return f ? state.windows.get(f.id) || null : null;
  };

  bus.on('window:closing', ({ id }) => {
    const rec = state.windows.get(id);
    if (!rec) return;
    if (rec.code) { rec.code.closed = true; closeCode(rec); }
    rec.chromeRo?.disconnect();
    rec.zoneGuard?.detach();
    state.windows.delete(id);

    geometry.delete(rec.ownerId);
    rec.host?.destroy?.();
    renderTaskbar();
  });
  for (const name of ['window:focused', 'window:minimized', 'window:restored', 'window:maximized', 'window:closed', 'window:opened']) {
    bus.on(name, () => queueMicrotask(renderTaskbar));
  }

  function focusOrToggle(winId) {
    const d = wm.describe(winId);
    if (!d) return;
    if (d.isFocused && d.state !== 'minimized') wm.minimize(winId);
    else wm.focus(winId);
    renderTaskbar();
  }

  function onPinned(appId, anchor) {
    const recs = [...state.windows.values()].filter((r) => r.appId === appId);
    if (!recs.length) { openApp(appId, { anchor }); return; }
    const top = recs.map((r) => ({ r, d: wm.describe(r.id) })).sort((a, b) => Number(b.r.handle.element.style.zIndex) - Number(a.r.handle.element.style.zIndex))[0];
    focusOrToggle(top.r.id);
  }

  // FR6b (M-18): „Desktop“ wie Windows „Desktop anzeigen“: minimiert ALLE sichtbaren Fenster und merkt sie sich; der
  // nächste Klick (Strg+Alt+D) stellt genau diese Fenster in ihrer Stapelreihenfolge wieder her. Ist nichts sichtbar
  // und nichts gemerkt, bleibt der Desktop (FR6d: eigene Fläche desktop.js) und bekommt den Fokus. Das Startmenü
  // schließt sich vorher selbst (launcher.js, Fuß „Desktop“).
  function showDesktop() {
    const list = windowList();
    const visible = list.filter((w) => w.state !== 'minimized');
    if (visible.length) {
      state.desktopRestore = visible.sort((a, b) => a.z - b.z).map((w) => w.id);
      wm.minimizeAll();
    } else {
      const back = (state.desktopRestore || []).filter((id) => list.some((w) => w.id === id && w.state === 'minimized'));
      state.desktopRestore = null;
      if (back.length) for (const id of back) wm.focus(id);
      else state.desktop?.element.querySelector('[data-v4-desktop-surface]')?.focus({ preventScroll: true });
    }
    renderTaskbar();
  }
  state.showDesktop = showDesktop;
  // ICON25 (V2-Vertrag window-manager.js prepareShellV2IconGeometry): Fenster-Icon und Launcher teilen EINE gerenderte
  // Glyphengröße (Desktop-Launcher, 64 px). Ein kleiner Ursprung (Startmenü-Eintrag, Reiter, Crew-Leiste) setzte sonst
  // --shell-v2-icon-size auf seine eigenen ~25 px. Der Anker behält die Klickposition (Öffnungsanimation), nimmt aber die
  // Glyphengröße des Desktops; fehlt sie (Desktop nicht gelayoutet), gilt 64 px wie in V2.
  function launcherGlyphSize() {
    const g = state.desktop?.element?.querySelector('.desktop-icon-glyph');
    const w = g ? g.getBoundingClientRect().width : 0;
    return w >= 24 && w <= 128 ? w : 64;
  }
  function launcherSizedAnchor(el) {
    if (!el) return undefined;
    return () => {
      const r = el.getBoundingClientRect();
      const size = launcherGlyphSize();
      if (Math.abs(r.width - size) <= 1 && Math.abs(r.height - size) <= 1) return r;
      const cx = r.left + r.width / 2;
      const cy = r.top + r.height / 2;
      return { left: cx - size / 2, top: cy - size / 2, width: size, height: size, radius: size * 0.28 };
    };
  }
  // WJB-D (Mobil-Host, location.hash = Desktop): Desktop zeigen OHNE Umschalten – sichtbare Fenster minimieren, sonst
  // nur den Desktop fokussieren (kein Zurückholen gemerkter Fenster).
  state.showDesktopOnly = () => {
    if (windowList().some((w) => w.state !== 'minimized')) showDesktop();
    else state.desktop?.element.querySelector('[data-v4-desktop-surface]')?.focus({ preventScroll: true });
  };

  // FR6b: Kürzel der Rahmen-Leisten. Strg+Alt+D = Desktop anzeigen/zurück (kein Browser-Standard, Labor FR6b-D3);
  // Strg+Alt+B (Mac: Ctrl+Opt+B) = Problem melden im vorderen App-Fenster (../shell/report.js).
  const onShellKey = (event) => {
    if (event.repeat || event.metaKey) return;
    if (event.ctrlKey && event.altKey && !event.shiftKey && event.code === 'KeyD') {
      event.preventDefault();
      showDesktop();
    } else if (event.ctrlKey && event.altKey && !event.shiftKey && event.code === 'KeyB') {
      const rec = focusedRec();
      if (!rec || rec.view) return;
      event.preventDefault();
      reportProblem({ appId: rec.appId, windowId: rec.ownerId, kind: 'bug' });
    }
  };
  document.addEventListener('keydown', onShellKey);
  state.unsubs.push(() => document.removeEventListener('keydown', onShellKey));

  function cycleWindows(backwards = false) {
    const list = windowList().sort((a, b) => b.z - a.z);
    if (list.length < 2) return;
    const next = backwards ? list[list.length - 1] : list[1];
    wm.focus(next.id);
    renderTaskbar();
  }

  // Andock-Kürzel wie macOS (Strg+Wahl+Pfeil = Hälften, Strg+Wahl+Enter = maximieren, U/I/J/K = Viertel) und
  // Vollbild (F11, Strg+Cmd+F) für das fokussierte Fenster.
  const SNAP_KEYS = { ArrowLeft: 'left', ArrowRight: 'right', ArrowUp: 'top', ArrowDown: 'bottom', KeyU: 'top-left', KeyI: 'top-right', KeyJ: 'bottom-left', KeyK: 'bottom-right' };
  const onKey = (event) => {
    if (event.altKey && !event.ctrlKey && (event.code === 'Backquote' || event.key === '`')) {
      event.preventDefault();
      cycleWindows(event.shiftKey);
      return;
    }
    const rec = focusedRec();
    if (!rec) return;
    // Strg/Cmd+Umschalt+E: App ↔ Code im fokussierten Fenster (nur mit apps.source.view).
    if ((event.ctrlKey || event.metaKey) && event.shiftKey && !event.altKey && event.code === 'KeyE' && rec.code && canViewSource() && sourceKernel) {
      event.preventDefault();
      if (rec.handle.element.dataset.v4Mode === 'code') showApp(rec); else showCode(rec);
      return;
    }
    if (event.ctrlKey && event.altKey && !event.metaKey && (SNAP_KEYS[event.code] || event.key === 'Enter')) {
      event.preventDefault();
      if (event.key === 'Enter') { if (wm.describe(rec.id)?.state === 'maximized') rec.handle.restore(); else rec.handle.maximize(); }
      else rec.handle.snapTo(SNAP_KEYS[event.code]);
      renderTaskbar();
    } else if (event.key === 'F11' || (event.ctrlKey && event.metaKey && event.code === 'KeyF')) {
      event.preventDefault();
      if (isFullscreen(rec)) exitFullscreen(rec); else enterFullscreen(rec);
    }
  };

  document.addEventListener('keydown', onKey);
  state.unsubs.push(() => document.removeEventListener('keydown', onKey));

  state.openApp = openApp;
  state.navigateTo = navigateTo;
  state.setLauncherOrder = setLauncherOrder;
  // FR6d: Startmenü (Popup aus dem Raster-Knopf, launcher.js) über allem, Desktop (desktop.js) als Fläche hinter den
  // Fenstern, Crew-Leiste (crew-bar.js) unten links.
  const launcher = createLauncher({
    host: root,
    isStartButton: (node) => !!node?.closest?.('[data-shell-start]'),
    t,
    iconFor,
    isPinned: (id) => state.pins.includes(id),
    onTogglePin: togglePin,
    onOpen: (appId, { anchor }) => openApp(appId, { anchor }),
    order: state.launcherOrder,
    onReorder: (ids) => setLauncherOrder(ids),
    // FR6c (M-18): Fuß wie das Startmenü der Shell-V2 – „Desktop“ minimiert alle Fenster bzw. stellt sie wieder her.
    onDesktop: () => showDesktop(),
    onSettings: () => state.topbar?.openSettings(),
  });
  const desktop = createDesktop({
    host: desk,
    before: layer,
    t,
    iconFor,
    order: state.launcherOrder,
    onOpen: (appId, { anchor }) => openApp(appId, { anchor }),
    onUnpin: (appId) => { if (state.pins.includes(appId)) togglePin(appId); },
  });
  state.desktop = desktop;
  // Crew-Mitglied anklicken = dessen Chat nach vorn (läuft er schon) bzw. öffnen; Pille „Crew“ = die Crew-Gruppe nach
  // vorn (oberstes Crew-Fenster, sonst das erste Mitglied).
  const crewApps = () => state.apps.filter((a) => taskGroupOf(a)?.key === 'crew');
  function openMember(appId, { anchor = null } = {}) {
    const recs = [...state.windows.values()].filter((r) => r.appId === appId && !r.view);
    if (!recs.length) { openApp(appId, { anchor }); return; }
    const top = recs.sort((a, b) => Number(b.handle.element.style.zIndex) - Number(a.handle.element.style.zIndex))[0];
    wm.focus(top.id);
    renderTaskbar();
  }
  const crewBar = createCrewBar({
    host: root,
    t,
    lang,
    iconFor,
    onMember: (appId, { anchor }) => openMember(appId, { anchor }),
    onCrew: ({ anchor }) => {
      const ids = new Set(crewApps().map((a) => a.id));
      const recs = [...state.windows.values()].filter((r) => ids.has(r.appId) && !r.view);
      if (recs.length) openMember(recs.sort((a, b) => Number(b.handle.element.style.zIndex) - Number(a.handle.element.style.zIndex))[0].appId);
      else if (ids.size) openMember(crewApps()[0].id, { anchor });
    },
  });
  // Beim Abtreten (Doppelpuffer) bleibt alles sichtbar stehen, bis der Rahmen entfernt wird; nur Menü und Beobachter enden.
  state.unsubs.push(() => { launcher.close({ restore: false }); desktop.disconnect(); });

  // ---------- CHAT4: Chatleiste der Shell-V2 (shared/business-chat.js unverändert, v4/shell/chat) ----------
  // Steht, solange die ctox-App aktiv ist: ihr gehören business_chats und der Befehl business_os.chat.task; ohne sie
  // hätte die Leiste weder Verläufe noch einen Empfänger. Montiert tritt die Crew-Leiste ab (eine Leiste unten links).
  // Ein neuer Rahmen (Doppelpuffer) übernimmt die Leiste im selben Schritt; die alte endet erst mit ihrem Rahmen.
  // getActiveModule = das fokussierte Fenster (Quelle des Auftrags, wie state.activeModule der Shell-V2).
  let chatLoading = null;
  const chatModule = () => {
    const rec = focusedRec();
    return rec && !rec.view ? { id: rec.appId, title: rec.title, name: rec.title } : null;
  };
  function syncBusinessChat() {
    const wanted = state.appById.has(CHAT_APP_ID);
    if (wanted && !state.chat && !chatLoading) {
      chatLoading = import('../shell/chat/index.js').then((m) => {
        chatLoading = null;
        if (state.stopped || state.chat || !state.appById.has(CHAT_APP_ID)) return;
        const chat = m.mountBusinessChat({ kernel, shellVer, getActiveModule: chatModule });
        if (!chat) return;
        state.chat = chat;
        crewBar.handOver(true);
      }).catch((e) => { chatLoading = null; console.warn('[v4 frame] business chat', e); });
    } else if (!wanted && state.chat) {
      state.chat.unmount();
      state.chat = null;
      crewBar.handOver(false);
    }
  }

  // ---------- „Neue App“ (Quelltext-Modus, neuer Entwurf mit base_app_ver = null) ----------
  // Rechte-Spiegel aus der Shell-App-API; ohne kernel.source(appId) gibt es keinen Einstieg.
  const sourceFn = typeof shellKernel.source === 'function' ? shellKernel.source.bind(shellKernel)
    : (typeof loaderKernel?.source === 'function' ? loaderKernel.source.bind(loaderKernel) : null);
  let actorPerms = null;
  try { actorPerms = kernel.appApi('shell', shellVer)?.actor?.permissions ?? null; } catch { actorPerms = null; }
  const permsNow = () => { try { return actorPerms?.get?.() ?? []; } catch { return []; } };
  const canCreateApp = () => !!sourceFn && Array.isArray(permsNow()) && permsNow().includes(PERM_SOURCE_EDIT);
  // Was der Quelltext-Modus vom Kernel braucht: source(appId), apps (mit active_ver), artifacts.manifest.
  // AGENT (§16.44): die Agent-Seitenleiste spricht über die App-API der Shell (ctox.coding.turn/cancel, coding_turns).
  const sourceKernel = sourceFn ? { source: sourceFn, apps: shellKernel.apps, artifacts: kernel.artifacts,
    shellAppApi: (() => { try { return kernel.appApi('shell', shellVer) ?? null; } catch { return null; } })() } : null;
  let sourceCss = null;
  function loadSourceCss() {
    if (!sourceCss) {
      // Link sofort in cssLinks eintragen: lädt er noch, während der Doppelpuffer umschaltet, darf das Aufräumen fremder
      // Shell-CSS ihn nicht entfernen (sonst feuert load nie und die Code-Ansicht bleibt nach dem Tausch leer).
      const link = document.createElement('link');
      link.rel = 'stylesheet';
      link.href = new URL('../source/source-mode.css', import.meta.url).href;
      link.dataset.v4FrameCss = '';
      cssLinks.push(link);
      sourceCss = new Promise((resolve) => {
        link.addEventListener('load', () => resolve(link), { once: true });
        link.addEventListener('error', () => resolve(link), { once: true });
        document.head.appendChild(link);
      });
    }
    return sourceCss;
  }

  function openNewApp({ anchor = null, restore = null } = {}) {
    if (!canCreateApp()) return null;
    const existing = [...state.windows.values()].find((r) => r.view === NEW_APP_VIEW);
    if (existing && !restore) { wm.focus(existing.id); renderTaskbar(); return existing; }
    const ownerId = restore?.ownerId || `v4view:new-app#${++state.seq}`;
    const seqMatch = /#(\d+)$/.exec(ownerId);
    if (seqMatch) state.seq = Math.max(state.seq, Number(seqMatch[1]));
    if (restore) pendingGeometry.set(ownerId, restore);
    const vp = wm.getViewport();
    const width = Math.min(560, Math.max(320, vp.w - vp.left - vp.right - 48));
    const height = Math.min(440, Math.max(240, vp.h - vp.top - vp.bottom - 48));
    const title = t('newApp', 'Neue App');
    const handle = wm.create({
      ownerId,
      title,
      width,
      height,
      x: Math.max(vp.left, Math.round(vp.left + (vp.w - vp.left - vp.right - width) / 2)),
      y: Math.max(vp.top, Math.round(vp.top + (vp.h - vp.top - vp.bottom - height) / 2)),
      minWidth: 300,
      minHeight: 220,
      shellGeometryContract: 'v4-frame',
      iconAnchorRect: launcherSizedAnchor(anchor),
    });
    pendingGeometry.delete(ownerId);
    handle.element.dataset.windowView = 'new-app';
    handle.element.dataset.windowId = ownerId;
    handle.container.classList.add('v4f-content', 'v4f-view');
    const zoneSlot = document.createElement('div');
    zoneSlot.className = 'v4f-zone-slot';
    zoneSlot.dataset.v4ZoneSlot = '';
    handle.container.after(zoneSlot);
    const zone = createStatusZone(zoneSlot, handle.element);
    const detachShellStatus = env.status?.attach?.(zone) || null;
    const rec = { id: handle.id, ownerId, appId: NEW_APP_VIEW, view: NEW_APP_VIEW, ver: '', appTitle: title, title, handle, host: null };
    let view = null;
    let closed = false;
    rec.host = {
      ver: '',
      zone,
      get view() { return view; },
      destroy() { closed = true; view?.close(); view = null; detachShellStatus?.(); zone.destroy?.(); },
    };
    state.windows.set(rec.id, rec);
    rec.host.ready = Promise.all([loadSourceCss(), import('../source/upload.js')]).then(([, mod]) => {
      if (closed || state.stopped) return;
      const host = document.createElement('div');
      host.className = 'v4-src-host';
      handle.container.append(host);
      view = mod.openAppUpload({ root: host, kernel: sourceKernel, api: { actor: { permissions: actorPerms } }, zone });
    }).catch((e) => zone.report({ level: 'error', code: 'source.open', text: `Neue App nicht ladbar: ${e?.message ?? e}`, source: 'shell' }));
    renderTaskbar();
    return rec;
  }
  // ---------- Quelltext einer App (api.shell.openSource, Standardschnittstelle für alle Apps) ----------
  // Öffnet ein Fenster des Rahmens (kein Overlay) mit dem Quelltext-Modus (../source/source-mode.js) für appId.
  // Nur mit apps.source.view (Spiegel; Lesen/Schreiben entscheidet der Kern, ohne apps.source.edit nur lesbar) und nur,
  // wenn der Kernel kernel.source(appId) hat und die App aktiv ist. Sonst null (der Host meldet es in die Zone).
  // Je App höchstens eine Quelltext-Ansicht: eine offene wird nach vorn geholt.
  const canViewSource = () => !!sourceFn && Array.isArray(permsNow()) && permsNow().includes(PERM_SOURCE_VIEW);
  // api.shell.openSource(appId): FR6a – kein eigenes Quelltext-Fenster mehr. Öffnet (bzw. holt) das App-Fenster und
  // schaltet es auf Code. Nur mit apps.source.view und kernel.source; sonst null (der Host meldet es in die Zone).
  function openSourceView(appId, { anchor = null, restore = null, focus = null } = {}) {
    const app = state.appById.get(String(appId || ''));
    if (!app || !canViewSource() || !sourceKernel) return null;
    const rec = restore ? openApp(app.id, { restore, anchor }) : openApp(app.id, { anchor });
    if (!rec) return null;
    wm.focus(rec.id);
    showCode(rec, focus);
    renderTaskbar();
    return rec;
  }

  function syncLauncherActions() {
    if (state.stopped) return;
    const allowed = canCreateApp();
    launcher.setActions(allowed ? [{ id: 'new-app', label: t('newApp', 'Neue App'), run: ({ anchor }) => openNewApp({ anchor }) }] : []);
    // Recht entzogen: offene Ansicht schließt (Spiegel; der Kern lehnt ohnehin ab).
    if (!allowed) for (const rec of [...state.windows.values()]) if (rec.view === NEW_APP_VIEW) wm.destroy(rec.id);
    // Ohne apps.source.view: Umschalter weg, Code-Ansicht schließt (Spiegel; der Kern lehnt ohnehin ab).
    const viewOk = canViewSource() && !!sourceKernel;
    for (const rec of state.windows.values()) {
      if (!rec.code) continue;
      if (!viewOk) closeCode(rec);
      else if (rec.code?.pending) { rec.code.pending = false; showCode(rec); }
    }
  }
  state.openNewApp = openNewApp;
  state.openSourceView = openSourceView;
  syncLauncherActions();
  if (actorPerms?.subscribe) state.unsubs.push(actorPerms.subscribe(() => queueMicrotask(syncLauncherActions)));
  // TOPBAR: Kopfleiste der Shell-V2 (topbar.js, CSS wörtlich aus app.css → shared/shell-v2-topbar.css). Reiter folgen
  // derselben Quelle wie die Taskleiste; Launcher-Raster = Desktop/Launcher; Zahnrad + Konto-Blatt → Einstellungen.
  {
    let actor = null;
    try { actor = kernel.appApi('shell', shellVer)?.actor ?? null; } catch { actor = null; }
    const topbar = createTopbar({
      host: root, kernel, lang, iconFor, prefs, actor, onLauncher: () => launcher.toggle(),
      onTab: (winId) => { if (winId && state.windows.has(winId)) { wm.focus(winId); renderTaskbar(); } },
      // FR6c: Reiter mit der FR6b-Logik (Pin öffnen, Einzelfenster fokussieren/minimieren, Gruppe reihum, Ziehen).
      onPinned, onWindow: focusOrToggle, onPinDrop: dropPin,
      onFocusWindow: (winId) => { if (state.windows.has(winId)) { wm.focus(winId); renderTaskbar(); } },
    });
    state.topbar = topbar;
    state.unsubs.push(() => { topbar.destroy(); if (state.topbar === topbar) state.topbar = null; });
  }
  // FR6b (M-06): Meldeschleife. reportProblem({appId, kind}) aus ../shell/report.js arbeitet gegen diesen Rahmen.
  state.unsubs.push(installReporter({
    window(appId, windowId) {
      const recs = [...state.windows.values()].filter((r) => !r.view && (windowId ? r.ownerId === windowId || r.id === windowId : r.appId === appId));
      const rec = recs.sort((a, b) => Number(b.handle.element.style.zIndex) - Number(a.handle.element.style.zIndex))[0];
      if (!rec || !rec.host?.win?.sheet) return null;
      return {
        appId: rec.appId,
        appTitle: rec.appTitle,
        title: rec.title,
        ver: rec.host.ver || rec.ver,
        windowId: rec.ownerId,
        element: rec.handle.element,
        content: rec.handle.container,
        sheet: rec.host.win.sheet,
        zone: rec.host.zone,
        focus: () => { wm.focus(rec.id); renderTaskbar(); },
      };
    },
    api: () => { try { return kernel.appApi('shell', shellVer); } catch { return null; } },
    shellVer,
  }));
  // ---------- Apps aus dem Kernel (gepusht) ----------
  function applyApps(rows) {
    state.rawApps = rows;
    for (const row of rows) loadManifest(row);
    // Shell und Plattform-Bibliotheken (lib:<name>, §16.37) sind keine Apps.
    state.apps = rows.filter((row) => row && row.id && row.id !== 'shell' && !String(row.id).startsWith('lib:') && row.activated_ver).map(appView);
    state.appById = new Map(state.apps.map((a) => [a.id, a]));
    state.resolveAppsLoaded?.();
    for (const cb of state.appsListeners || []) { try { cb(); } catch (e) { console.warn('[v4 frame] apps listener', e); } }
    launcher.setApps(state.apps);
    desktop.setApps(state.apps);
    crewBar.setMembers(crewApps());
    syncBusinessChat();
    for (const rec of [...state.windows.values()]) {
      if (rec.view) continue; // Shell-Ansicht, keine App
      const app = state.appById.get(rec.appId);
      if (!app) { wm.destroy(rec.id); continue; } // App fuer diesen Nutzer entfernt/entzogen
      const hostVer = rec.host?.ver || rec.ver;
      if (app.raw.activated_ver && app.raw.activated_ver !== hostVer && app.raw.state !== 'failed') swapWindow(rec, app.raw.activated_ver);
    }
    renderTaskbar();
  }

  // Live-Remount eines Fensters. meta.explicit = Aktivierungsereignis (darf eine zurückgefallene Version neu
  // versuchen), sonst Abgleich aus kernel.apps (wiederholt eine kaputte Version nicht im Takt).
  function swapWindow(rec, ver, meta = {}) {
    if (!ver) return;
    if (!meta.explicit && (rec.host?.ver || rec.ver) === ver) return;
    rec.ver = ver;
    rec.host?.swap?.(ver, meta);
  }

  // Erste Liste samt Manifesten (Icon, Kategorie, Einstieg) abwarten, damit wiederhergestellte Fenster
  // sofort mit Icon erscheinen; danach live.
  const firstRows = typeof kernel.apps.get === 'function' ? (kernel.apps.get() || []) : [];
  await Promise.race([
    Promise.all(firstRows.map((row) => loadManifest(row, { rerender: false }))),
    new Promise((resolve) => setTimeout(resolve, 1500)),
  ]);
  state.rendered = true;
  if (prefs) state.unsubs.push(prefs.subscribe(applyPreferences));
  state.unsubs.push(kernel.apps.subscribe((rows) => { if (!state.stopped) applyApps(Array.isArray(rows) ? rows : []); }));
  if (kernel.activation?.onChange) {
    state.unsubs.push(kernel.activation.onChange((ev) => {
      if (state.stopped || !ev) return;
      const ver = ev.activated_ver || ev.ver;
      if (ev.state === 'activated' && ver && !ev.reverted) state.brokenVer.delete(ev.app_id);
      for (const rec of state.windows.values()) {
        if (rec.appId !== ev.app_id) continue;
        rec.host?.activation?.(ev);
        if (ev.state === 'activated' && ver) swapWindow(rec, ver, { explicit: true, t0: ev.t0, reverted: ev.reverted });
      }
    }));
  }

  // ---------- Wiederherstellen (Shell-Tausch) ----------
  if (restored) {
    launcher.query = restored.search || '';
    const list = (restored.windows || []).slice().sort((a, b) => (a.z || 0) - (b.z || 0));
    const created = [];
    for (const entry of list) {
      const id = String(entry.app_id || '');
      const rec = id === NEW_APP_VIEW ? openNewApp({ restore: entry })
        : id.startsWith(`${SOURCE_VIEW}:`) ? openSourceView(id.slice(SOURCE_VIEW.length + 1), { restore: entry })
          : openApp(entry.app_id, { restore: entry });
      // Ältere Snapshots mit eigenem Quelltext-Fenster (shell:source:<app>) öffnen das App-Fenster im Code-Modus.
      if (rec && entry.mode === 'code' && rec.code && !showCode(rec)) rec.code.pending = true; // Rechte-Spiegel noch leer
      if (rec) created.push({ rec, entry });
    }
    for (const { rec, entry } of created) {
      if (entry.appMode && entry.appMode !== 'window') wm.setAppMode(rec.id, entry.appMode);
    }
    const focused = created.filter(({ entry }) => entry.state !== 'minimized').pop();
    if (focused) wm.focus(focused.rec.id);
    for (const { rec, entry } of created) if (entry.state === 'minimized') wm.minimize(rec.id);
    wm.finalizeDockRestore?.();
    renderTaskbar();
  }

  // Doppelpuffer: erst wenn alle wiederhergestellten Fenster montiert (oder zurückgefallen) sind, umschalten.
  if (retiring.length) {
    const hosts = [...state.windows.values()].map((r) => r.host?.ready).filter(Boolean);
    await Promise.race([Promise.allSettled(hosts), new Promise((resolve) => setTimeout(resolve, SWITCH_DEADLINE_MS))]);
  }
  if (state.stopped) return;
  // Umschalten in EINEM Schritt (synchron, kein Frame dazwischen).
  for (const old of retiring) old.dispatchEvent(new CustomEvent(RETIRE_EVENT));
  for (const old of retiring) old.remove();
  for (const link of document.querySelectorAll('link[data-v4-frame-css]')) if (!cssLinks.includes(link)) link.remove();
  root.style.opacity = '';
  root.style.pointerEvents = '';
  delete root.dataset.v4Building;
  for (const rec of state.windows.values()) rec.host?.refocus?.();
  state.startMs = performance.now() - startedAt;
  root.dataset.v4Ready = '';
}

export async function stopShell() {
  const state = current;
  if (!state) return null;
  const { wm } = state;
  const described = new Map(wm.listWindows().map((w) => [w.id, w]));
  const windows = [];
  for (const rec of state.windows.values()) {
    const el = rec.handle.element;
    const d = described.get(rec.id) || {};
    const sheet = el.classList.contains('is-mobile-sheet');
    const saved = state.geometry.get(rec.ownerId) || null;
    windows.push({
      ownerId: rec.ownerId,
      app_id: rec.appId,
      ver: rec.host?.ver || rec.ver,
      title: rec.title,
      x: sheet && saved ? saved.x : parsePx(el.style.left),
      y: sheet && saved ? saved.y : parsePx(el.style.top),
      width: sheet && saved ? saved.width : parsePx(el.style.width),
      height: sheet && saved ? saved.height : parsePx(el.style.height),
      state: d.state || 'normal',
      snapZone: el.dataset.snapZone || '',
      appMode: d.appMode || 'window',
      mode: rec.handle.element.dataset.v4Mode === 'code' ? 'code' : 'app',
      stored: d.state === 'maximized' || el.dataset.snapZone ? saved?.stored || null : null,
      z: Number(el.style.zIndex) || 0,
      focused: !!d.isFocused,
      app_snapshot: null,
    });
  }
  // App-Snapshots vom Fenster-Host (structured clone, <= 1 MiB je Fenster laut Vertrag).
  const recs = [...state.windows.values()];
  // Code-Ansicht (FR6a): laufende Entwurfs-Schreibvorgänge abschließen, bevor der neue Rahmen sie neu öffnet.
  await Promise.all(recs.map((rec) => Promise.resolve(rec.code?.view?.flush?.()).catch(() => null)));
  const snaps = await Promise.all(recs.map((rec) => Promise.resolve(rec.host?.snapshot?.()).catch(() => null)));
  snaps.forEach((snap, i) => { windows[i].app_snapshot = snap ?? null; });
  const focusOrder = windows.slice().sort((a, b) => a.z - b.z).map((w) => w.ownerId);
  const snapshot = {
    version: FRAME_SNAPSHOT_VERSION,
    windows,
    focus_order: focusOrder,
    pins: encodeTaskbarPinCache(state.pins, state.pinsUpdatedAtMs),
    launcher_order: state.launcherOrder.slice(),
    prefs_updated_at_ms: state.prefsUpdatedAtMs || 0,
    search: state.root.querySelector('[data-v4-search]')?.value || '',
    taken_at: Date.now(),
  };
  state.stopped = true;
  current = null;
  for (const unsub of state.unsubs) { try { unsub?.(); } catch { /* still */ } }
  // Fensteranordnung je Tab (§16.19 Nr. 3); der Loader bekommt denselben Snapshot als Rückgabewert.
  try { await state.kernel.windowState.put(SHELL_STATE_KEY, snapshot); } catch (e) { console.warn('[v4 frame] window_state', e); }
  // Doppelpuffer: der alte Rahmen bleibt sichtbar (eingefroren), bis die neue Shell umschaltet.
  // Danach (Ereignis v4:retire) werden seine Apps ohne Schliess-Animation abgebaut.
  const { root } = state;
  root.dataset.v4Retiring = '';
  root.style.pointerEvents = 'none';
  root.addEventListener(RETIRE_EVENT, () => {
    root.remove();
    state.chat?.unmount(); // CHAT4: nur, wenn kein neuer Rahmen die Leiste übernommen hat
    for (const rec of recs) Promise.resolve(rec.host?.destroy?.({ keepState: true })).catch(() => {});
  }, { once: true });
  return snapshot;
}

// K-WJ-D1: Mobil-Katalog für die Workjet-Mobil-Hostbrücke in V2-Form (workjet.business-os-mobile-apps.v1). Quelle ist
// layout.presentation.mobile je App (gen-manifest übernimmt die V2-Einträge als Daten); Apps ohne Eintrag fehlen wie
// in V2. Die Revision folgt dem Inhalt, damit Workjet einen geänderten Katalog erkennt.
export function mobileCatalogOf(apps) {
  const list = (apps || []).filter((a) => a && a.id !== 'desktop' && a.layout?.presentation?.mobile && typeof a.layout.presentation.mobile === 'object')
    .map((a) => ({ id: a.id, ...a.layout.presentation.mobile }));
  let h = 0x811c9dc5;
  for (const ch of JSON.stringify(list)) { h ^= ch.charCodeAt(0); h = Math.imul(h, 0x01000193) >>> 0; }
  return { type: 'workjet.business-os-mobile-apps.v1', revision: `ctox-shell-v4-${h.toString(16).padStart(8, '0')}`, apps: list };
}

// Labor-/Test-Zugriff auf die laufende Shell (kein Loader-Vertrag).
export function shellHandle() {
  const state = current;
  if (!state) return null;
  const byOwner = (ownerId) => [...state.windows.values()].find((r) => r.ownerId === ownerId) || null;
  return {
    root: state.root,
    kernel: state.kernel,
    setLauncherOrder: (ids) => state.setLauncherOrder(ids),
    preferencesWritten: () => state.prefsWrite || Promise.resolve(),
    navigate: (appId, params) => state.navigateTo(appId, params),
    showDesktop: () => state.showDesktopOnly(),
    openWindow(appId, opts = {}) { const rec = state.openApp(appId, { multi: true, ...opts }); return rec ? rec.ownerId : null; },
    openNewApp() { const rec = state.openNewApp(); return rec ? rec.ownerId : null; },
    openSource(appId, focus = null) { const rec = state.openSourceView(appId, { focus }); return rec ? rec.ownerId : null; },
    windows: () => [...state.windows.values()],
    closeWindow(ownerId) { const rec = byOwner(ownerId); if (rec) state.wm.destroy(rec.id); return !!rec; },
    win: byOwner,
    geometry(ownerId) {
      const el = byOwner(ownerId)?.handle.element;
      return el ? { x: parsePx(el.style.left), y: parsePx(el.style.top), w: parsePx(el.style.width), h: parsePx(el.style.height) } : null;
    },
  };
}

export function describeShell() {
  if (!current) return null;
  return {
    startMs: current.startMs,
    apps: current.apps.map((a) => a.id),
    pins: current.pins.slice(),
    launcherOrder: current.launcherOrder.slice(),
    preferencesActor: current.prefs?.actorId || null,
    windows: [...current.windows.values()].map((r) => ({ id: r.id, app: r.appId, ver: r.host?.ver || r.ver, state: r.host?.state || null })),
    gaps: [...current.kernel.gaps],
    failedReports: current.kernel.failedReports.slice(),
  };
}
