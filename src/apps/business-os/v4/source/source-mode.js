// Shell v4 – Quelltext-Modus im App-Fenster (Entwurf §8; Architektur §9.2, §10.2a Nr. 7, §16.4, §16.10, §16.22).
//
//   openSourceMode({ root, appRecord, kernel, api, zone?, editor? }) → Promise<{ close(), el, flush(), editor, state }>
//     root      Element im App-Fenster (nie außerhalb), das die Ansicht aufnimmt
//     appRecord AppEntry aus kernel.apps ({ id, title, active_ver })
//     kernel    Shell-Kernel-API: source(appId), apps (LiveQuery), artifacts.manifest(appId, ver)
//     api       App-API des Fensters oder ein stabiler Spiegel davon: actor.permissions (LiveDoc<string[]>), status
//     zone      Zustandszone des Fensters ({ report, clear }); fehlt sie, geht es über api.status
//     editor    'auto' (Monaco aus vendor/, sonst textarea) | 'textarea'
//     sheet     Fensterblatt (../frame/window-sheet.js) für die Versionsnotiz beim Veröffentlichen (Strang LOOP)
//     focus     { tab: 'files' | 'reports', reportId? } aus api.shell.openSource(appId, options) (Strang LOOP Runde 2):
//               öffnet auf dem Reiter, reportId markiert die Meldung; später erneut: handle.focus(focus)
//
// Meldeschleife (Strang LOOP, Bedienkonzept §3a): neben dem Dateibaum der Reiter „Meldungen“ (reports-tab.js) mit den
// offenen Meldungen der App; „Agent beauftragen“ = ctox.coding.turn (§16.44) auf den eigenen Entwurf mit Bezug auf die
// Meldung (record_id), Meldung „in Arbeit“ + turn_id. „Veröffentlichen“ mit verknüpften Meldungen fragt im Fensterblatt
// die Versionsnotiz ab (Vorschlag „behebt Meldung #N“); jede in der Notiz genannte Meldung steht danach auf behoben.
//
// Alles läuft über die Quelltext-API des Kerns (§16.22, §16.26, Stub-Pin 9fa9948), keine eigene Entwurfshaltung:
//   draft.info()            beim Öffnen und nach jeder Kernaktion (Basis + Dateien + Manifest-Felder des EIGENEN Entwurfs)
//   draft.write/remove/rename  Dateien (Löschen/Umbenennen im Dateibaum); rename zieht entry/handlers/styles im Kern mit
//   draft.setCollections(text) collections.schema.json; der zurückgegebene Diff ist maßgeblich (lokale Einstufung aus
//                           schema-diff.js nur als Vorschau beim Tippen und für Nur-Lesen; Abweichungen → state.deviations)
//   draft.setManifest(fields)  einzige Quelle der Manifest-Felder (§16.26): manifest.json ist im Entwurf keine Datei und
//                           erscheint nicht im Dateibaum; eine hochgeladene manifest.json wird zu setManifest
//   draft.publish({ base_app_ver })  Vergleich-und-Setzen im Kern (prüft auch Verweise auf fehlende Dateien → invalid mit
//                           detail in die Zone); stale_base → Zone mit Aktion „Auf aktuelle Version setzen“
//                           (= draft.rebase(aktiv)); „Verwerfen“ = draft.discard()
// Rechte sind ein Spiegel (Entscheidung im Kern): ohne apps.source.view schließt die Ansicht, ohne apps.source.edit
// ist sie nur lesbar. Keine Zustandstexte in der Ansicht selbst: Fehler, Konflikte und Ablehnungen gehen
// ausschließlich in die Zone.

import { createEditor, languageOf } from './editor.js';

// K-1k (§16.50 Nr. 3–6): Monaco-Worker gleich-ursprünglich unter der Lib-Bindung der Shell (kernel.artifacts.bind('shell',
// <shell-ver>).workerUrl). Ohne Shell-Instanz-URL (Labor-Dateiserver) oder ohne bind/workerUrl bleibt editor.js beim
// ctox:lib-Literal, das der Service Worker umschreibt.
const SHELL_VER = (/\/shell@([^/~]+)[/~]/.exec(import.meta.url) || [])[1] || null;
function shellWorkerUrl(kernel) {
  if (!SHELL_VER || typeof kernel?.artifacts?.bind !== 'function') return null;
  try {
    const b = kernel.artifacts.bind('shell', SHELL_VER);
    return typeof b?.workerUrl === 'function' ? (lib, path) => b.workerUrl(lib, path) : null;
  } catch { return null; }
}
import { createFolderInput } from './upload.js';
import { createAgentPanel } from './agent-panel.js';
import { createCodingPort } from './coding-port.js';
import { createReportsTab } from './reports-tab.js';
import { codingPromptFor, fixNoteFor, notedNos, reportNo, linkReportToTurn, markReportsFixed, recentZoneErrors } from '../shell/report.js';
import { renderDiff as renderTextDiff } from './text-diff.js';
import { createZoneGuard } from '../frame/pane-heads.js';
import {
  parseCollectionsFile, classifyCollections, publishBlockers, migrationPath, migrationTemplate, withMigration,
} from './schema-diff.js';

export const SCHEMA_FILE = 'collections.schema.json';
export const MANIFEST_FILE = 'manifest.json';
export const PERM_VIEW = 'apps.source.view';
export const PERM_EDIT = 'apps.source.edit';
const MANIFEST_VIEW = '::manifest';
const WRITE_DEBOUNCE_MS = 250;
const KIND_LABEL = { compatible: 'kompatibel', new: 'kompatibel · neu', generation: 'neue Generation', removed: 'entfernt', unchanged: 'unverändert' };
// Manifest: Formularfelder, vom Kern verwaltete Felder (nie anzeigen/senden). Alles andere = „Weitere Felder“.
const FORM_KEYS = ['title', 'entry', 'min_app_api', 'capabilities_required', 'handlers'];
const SYSTEM_KEYS = new Set(['app_id', 'app_ver', 'files', 'collections', 'generations', 'retires', 'provenance', 'author', 'created_at']);

let cssLinked = false;
function linkCss() {
  if (cssLinked || typeof document === 'undefined') return;
  const href = new URL('./source-mode.css', import.meta.url).href;
  if (!document.querySelector(`link[href="${href}"]`)) {
    const l = document.createElement('link');
    l.rel = 'stylesheet';
    l.href = href;
    l.dataset.shellCss = 'source';
    document.head.append(l);
  }
  cssLinked = true;
}

export const canViewSource = (perms) => Array.isArray(perms) && perms.includes(PERM_VIEW);
export const canEditSource = (perms) => Array.isArray(perms) && perms.includes(PERM_EDIT);

function h(tag, cls, text) {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined) n.textContent = text;
  return n;
}
const isObj = (v) => !!v && typeof v === 'object' && !Array.isArray(v);
const canon = (v) => (Array.isArray(v) ? `[${v.map(canon).join(',')}]` : isObj(v) ? `{${Object.keys(v).sort().map((k) => `${JSON.stringify(k)}:${canon(v[k])}`).join(',')}}` : JSON.stringify(v ?? null));
const same = (a, b) => canon(a) === canon(b);
// RFC-7396-Merge wie im Kern (setManifest): null entfernt.
function mergePatch(target, patch) {
  if (!isObj(patch)) return structuredClone(patch);
  const out = isObj(target) ? { ...target } : {};
  for (const [k, v] of Object.entries(patch)) { if (v === null) delete out[k]; else out[k] = mergePatch(out[k], v); }
  return out;
}

export async function openSourceMode({ root, appRecord, kernel, api, zone, editor: prefer = 'auto', agentOpen = true, sheet = null, focus = null }) {
  // Agent-Seitenleiste (unten belegt). Früh deklariert: syncButtons() läuft schon während des Aufbaus, ein späteres `let`
  // wirft dort ReferenceError (TDZ, „Cannot access 'agent' before initialization“).
  let agent = null;
  linkCss();
  const appId = appRecord.id;
  const src = kernel.source(appId);
  const sink = zone ?? { report: (i) => api.status.report(i), clear: (c) => api.status.clear(c) };
  const perms = () => api.actor.permissions.get() ?? [];
  const appEntry = () => kernel.apps.get().find((a) => a.id === appId) ?? null;
  const activeOf = () => appEntry()?.active_ver ?? null;

  const s = {
    base: null, basePaths: [], baseText: new Map(), baseManifest: null,
    draft: new Map(), // Pfad → Text | null (null = im Entwurf gelöscht); Spiegel von draft.info().files + laufende Eingaben
    manifestPatch: {}, pendingManifest: null, manifestTimer: null, manifestInputError: null,
    current: null, ed: null, timers: new Map(), writes: new Set(), writeResults: [],
    diff: [], diffSource: 'local', blockers: [], schemaError: null, deviations: [],
    publishing: false, closed: false, diffMode: false,
  };

  const el = h('div', 'v4-src');
  el.dataset.v4Source = appId;
  const bar = h('div', 'v4-src__bar');
  const fileSelect = h('select', 'v4-src__select');
  fileSelect.setAttribute('aria-label', 'Datei');
  const narrowActs = h('span', 'v4-src__acts v4-src__acts--narrow');
  const spacer = h('span', 'v4-src__spacer');
  // Editor-Kopf (Bedienkonzept §3): Hauptaktion „Veröffentlichen“, daneben „Verwerfen“, Diff als Umschalter,
  // alles Weitere im „…“-Menü (Ordner laden, Agent ein-/ausblenden).
  const diffBtn = h('button', 'v4-src__btn v4-src__toggle', 'Diff');
  diffBtn.type = 'button';
  diffBtn.dataset.diffToggle = '';
  diffBtn.title = 'Änderungen gegen die Basis zeigen';
  diffBtn.setAttribute('aria-pressed', 'false');
  const moreWrap = h('span', 'v4-src__more');
  const moreBtn = h('button', 'v4-src__btn v4-src__more-btn', '…');
  moreBtn.type = 'button';
  moreBtn.dataset.srcMore = '';
  moreBtn.title = 'Weitere Aktionen';
  moreBtn.setAttribute('aria-label', 'Weitere Aktionen');
  moreBtn.setAttribute('aria-haspopup', 'menu');
  moreBtn.setAttribute('aria-expanded', 'false');
  const moreMenu = h('div', 'v4-src__menu');
  moreMenu.setAttribute('role', 'menu');
  moreMenu.hidden = true;
  const uploadBtn = h('button', 'v4-src__menu-item', 'Ordner in den Entwurf laden…');
  uploadBtn.type = 'button';
  uploadBtn.dataset.srcUpload = '';
  uploadBtn.setAttribute('role', 'menuitem');
  const agentItem = h('button', 'v4-src__menu-item', 'Agent ausblenden');
  agentItem.type = 'button';
  agentItem.dataset.srcAgentItem = '';
  agentItem.setAttribute('role', 'menuitem');
  moreMenu.append(uploadBtn, agentItem);
  moreWrap.append(moreBtn, moreMenu);
  const discardBtn = h('button', 'v4-src__btn', 'Verwerfen');
  discardBtn.type = 'button';
  discardBtn.dataset.discard = '';
  const publishBtn = h('button', 'v4-src__btn v4-src__btn--primary', 'Veröffentlichen');
  publishBtn.type = 'button';
  publishBtn.dataset.publish = '';
  bar.append(fileSelect, narrowActs, h('span', 'v4-src__break'), spacer, diffBtn, moreWrap, discardBtn, publishBtn);
  const main = h('div', 'v4-src__main');
  const tree = h('nav', 'v4-src__tree');
  tree.setAttribute('aria-label', 'Dateien');
  // Reiter „Dateien“ | „Meldungen“ oben im Baum (nur mit Melde-Collections über die Shell-App-API).
  const sideTabs = h('div', 'v4-src__tabs');
  sideTabs.setAttribute('role', 'tablist');
  sideTabs.hidden = true;
  const filesTabBtn = h('button', 'v4-src__tab is-on', 'Dateien');
  const reportsTabBtn = h('button', 'v4-src__tab', 'Meldungen');
  const reportsCount = h('span', 'v4-src__tab-count');
  reportsTabBtn.append(reportsCount);
  for (const [b, k] of [[filesTabBtn, 'files'], [reportsTabBtn, 'reports']]) { b.type = 'button'; b.dataset.srcTab = k; b.setAttribute('role', 'tab'); b.setAttribute('aria-selected', String(k === 'files')); }
  sideTabs.append(filesTabBtn, reportsTabBtn);
  const filesEl = h('div', 'v4-src__files');
  tree.append(sideTabs, filesEl);
  const pane = h('div', 'v4-src__pane');
  main.append(tree, pane);
  el.append(bar, main);
  const folderInput = createFolderInput(async (up) => {
    for (const [p, text] of Object.entries(up.files)) {
      if (p !== MANIFEST_FILE) { setDraft(p, text); continue; }
      let fields;
      try { fields = JSON.parse(text); if (!isObj(fields)) throw new Error('JSON-Objekt erwartet'); } catch (e) {
        report('source.manifest', `${MANIFEST_FILE} aus dem Ordner nicht übernommen: ${errText(e)}`);
        continue;
      }
      queueManifest(manifestPatchFrom(fields));
    }
    await flush();
    renderTree();
    openPath(s.current && (s.current === MANIFEST_VIEW || livePaths().includes(s.current)) ? s.current : defaultPath());
  });
  el.append(folderInput);
  root.append(el);
  // ZONE2 (Befund T3-1): Der Kopf (Veröffentlichen, Verwerfen, Diff, …) liegt nie unter dem schwebenden Chrome – auch
  // dann nicht, wenn ein Wirt die Code-Ansicht ohne eigenen Zonen-Abstand ins Fenster legt. Die Zonen-Garantie des
  // Rahmens prüft die Ansicht selbst; schneidet etwas eine Zone, beginnt sie unter --shell-zone-height (source-mode.css).
  const zoneGuard = createZoneGuard();
  zoneGuard.attach(el);

  const report = (code, text, extra = {}) => sink.report({ level: 'error', code, text, ...extra });
  const errText = (e) => e?.message ?? String(e);

  // ---- Daten ---------------------------------------------------------------------------------------------------
  // manifest.json ist im Entwurf keine Datei (§16.26): weder im Dateibaum noch als Entwurfsänderung (der Kern trägt sie
  // beim Anlegen des Entwurfs als gelöscht ein, wenn die Basis sie noch hat).
  function livePaths() {
    const set = new Set(s.basePaths);
    for (const [p, t] of s.draft) { if (t === null) set.delete(p); else set.add(p); }
    set.delete(MANIFEST_FILE);
    return [...set].sort((a, b) => a.localeCompare(b));
  }
  const baseTextOf = (path) => (s.basePaths.includes(path) ? s.baseText.get(path) ?? '' : '');
  const textOf = (path) => (typeof s.draft.get(path) === 'string' ? s.draft.get(path) : baseTextOf(path));
  const changed = (path) => path === MANIFEST_VIEW
    ? Object.keys(s.manifestPatch).length > 0 || !!s.pendingManifest
    : typeof s.draft.get(path) === 'string' && (!s.basePaths.includes(path) || s.baseText.get(path) !== s.draft.get(path));
  const hasChanges = () => s.draft.size > 0 || Object.keys(s.manifestPatch).length > 0 || !!s.pendingManifest;

  async function loadBase(ver) {
    s.base = ver ?? null;
    s.baseText.clear();
    s.basePaths = s.base ? await src.list(s.base) : [];
    s.baseManifest = s.base && kernel.artifacts?.manifest ? await kernel.artifacts.manifest(appId, String(s.base)) : null;
    el.dataset.baseVer = s.base ?? '';
    publishBtn.dataset.base = s.base ?? '';
    await Promise.all(s.basePaths.map(async (p) => s.baseText.set(p, await src.read(p, s.base))));
  }
  /** Eigener Entwurf aus dem Kern (A4). Liefert info oder null. */
  async function loadDraft() {
    const info = await src.draft.info();
    s.draft = new Map(Object.entries(info?.files ?? {}).filter(([p]) => p !== MANIFEST_FILE));
    s.manifestPatch = structuredClone(info?.manifest ?? {});
    s.pendingManifest = null;
    return info;
  }
  /** Ansicht vollständig aus dem Kern neu aufbauen (nach publish, discard, rebase, neuer aktiver Version). */
  async function reload(fallbackVer) {
    const keep = s.current;
    const info = await loadDraft();
    await loadBase(info ? info.base_app_ver : (fallbackVer ?? activeOf()));
    if (info && typeof s.draft.get(SCHEMA_FILE) === 'string' && canEditSource(perms())) await pushCollections();
    else previewDiff();
    renderTree();
    await openPath(keep && (keep === MANIFEST_VIEW || livePaths().includes(keep)) ? keep : defaultPath());
    syncButtons();
  }

  function setDraft(path, text) {
    s.draft.set(path, text);
    clearTimeout(s.timers.get(path));
    s.timers.set(path, setTimeout(() => writeNow(path), WRITE_DEBOUNCE_MS));
    markTree(path);
    syncButtons();
  }
  function track(p) { s.writes.add(p); p.finally(() => s.writes.delete(p)).catch(() => {}); return p; }
  function writeNow(path) {
    clearTimeout(s.timers.get(path));
    s.timers.delete(path);
    if (!canEditSource(perms()) || typeof s.draft.get(path) !== 'string') return Promise.resolve();
    if (path === SCHEMA_FILE) return track(pushCollections());
    const handle = src.draft.write(path, s.draft.get(path));
    // Kern ab Stub-Pin 8489b4e: DraftWriteHandle mit result → {status:'accepted', rev} | {status:'rejected', code}.
    if (handle && typeof handle.result?.then === 'function') s.writeResults.push(handle.result);
    return track(Promise.resolve(handle).catch((e) => report('source.draft', `Entwurf nicht gespeichert (${path}): ${errText(e)}`)));
  }
  async function flush() {
    for (const path of [...s.timers.keys()]) writeNow(path);
    if (s.manifestTimer) track(flushManifest());
    while (s.writes.size) await Promise.all([...s.writes]);
  }

  // ---- Collections: Kern-Diff maßgeblich ------------------------------------------------------------------------
  function baseSchema() {
    // Wie der Kern (baseSchemaOf): Datei der Basis, sonst manifest.collections der Basis.
    if (s.basePaths.includes(SCHEMA_FILE)) return parseCollectionsFile(baseTextOf(SCHEMA_FILE));
    return parseCollectionsFile({ collections: s.baseManifest?.collections ?? {} });
  }
  function localDiff() {
    const next = livePaths().includes(SCHEMA_FILE) ? parseCollectionsFile(textOf(SCHEMA_FILE)) : baseSchema();
    return classifyCollections(baseSchema(), next, { baseVer: s.base });
  }
  /** Vorschau beim Tippen / Nur-Lesen: lokale Einstufung (gleiche Regeln wie der Kern, siehe schema-diff.test.mjs). */
  function previewDiff() {
    if (!livePaths().includes(SCHEMA_FILE) && !s.baseManifest?.collections) { s.diff = []; s.blockers = []; s.schemaError = null; return; }
    try {
      s.diff = localDiff();
      s.diffSource = 'local';
      s.schemaError = null;
    } catch (e) {
      s.schemaError = errText(e);
    }
    s.blockers = s.schemaError ? [{ name: SCHEMA_FILE, reason: 'invalid' }] : publishBlockers(s.diff, livePaths());
    renderDiff();
    syncButtons();
  }
  /** Schema-Text an den Kern (A6): schreibt die Datei in den Entwurf und liefert den maßgeblichen Diff. */
  async function pushCollections() {
    if (!canEditSource(perms())) return;
    const text = textOf(SCHEMA_FILE);
    try {
      const r = await src.draft.setCollections(text);
      s.schemaError = null;
      sink.clear('source.schema');
      s.diff = r.diff;
      s.diffSource = 'kernel';
      s.blockers = publishBlockers(r.diff, livePaths());
      logDeviations(r.diff);
    } catch (e) {
      if (e?.code !== 'invalid') { report('source.draft', `Collections-Entwurf nicht gespeichert: ${errText(e)}`); return; }
      // Ungültig: Text trotzdem im Entwurf halten (nichts verlieren), Veröffentlichen gesperrt.
      s.schemaError = errText(e);
      s.blockers = [{ name: SCHEMA_FILE, reason: 'invalid' }];
      report('source.schema', `${SCHEMA_FILE} ist ungültig: ${s.schemaError}`);
      await src.draft.write(SCHEMA_FILE, text).catch(() => {});
    }
    renderDiff();
    syncButtons();
  }
  function logDeviations(kernelDiff) {
    let local;
    try { local = localDiff(); } catch { return; }
    const byName = new Map(local.map((d) => [d.name, d]));
    for (const k of kernelDiff) {
      const l = byName.get(k.name);
      for (const f of ['kind', 'needsMigration', 'reasons', 'migration']) {
        if (l && same(l[f], k[f])) continue;
        const dev = { name: k.name, field: l ? f : 'missing', shell: l ? l[f] ?? null : null, kernel: k[f] ?? null };
        if (!s.deviations.some((x) => same(x, dev))) { s.deviations.push(dev); console.warn('[v4/source] Diff-Abweichung Shell ↔ Kern', dev); }
        if (!l) break;
      }
    }
  }

  // ---- Manifest (A5/§16.26: setManifest ist die einzige Quelle) -------------------------------------------------
  // Wirksames Manifest = Basis-Manifest, darüber die setManifest-Felder des Entwurfs (fehlende erbt die Basis).
  // handlers im Entwurf ist die vollständige Karte (der Kern patcht gegen Basis + Entwurf, null entfernt auch Basis-Handler).
  function baseManifestFields() {
    const bm = s.baseManifest ?? {};
    const out = {
      title: appEntry()?.title || appRecord.title || appId, entry: bm.entry ?? 'index.js', min_app_api: bm.min_app_api ?? '4.0',
      capabilities_required: bm.capabilities_required ?? [], handlers: bm.handlers ?? {},
    };
    for (const [k, v] of Object.entries(bm)) if (!FORM_KEYS.includes(k) && !SYSTEM_KEYS.has(k)) out[k] = v;
    if (out.layout && Object.values(out.layout).every((v) => v === null)) delete out.layout;
    if (Array.isArray(out.styles) && !out.styles.length) delete out.styles;
    return out;
  }
  /** Was der Kern veröffentlichen wird: Basis + Entwurfsfelder + noch nicht gesendete Eingaben (gleiche Patch-Regeln). */
  function effectiveManifest() {
    const out = { ...baseManifestFields(), ...s.manifestPatch };
    if (!s.pendingManifest) return out;
    const { handlers, ...rest } = s.pendingManifest;
    const next = mergePatch(out, rest);
    if (handlers !== undefined) next.handlers = handlers === null ? {} : mergePatch(out.handlers ?? {}, handlers);
    return next;
  }
  /** Patch für setManifest aus einem vollständigen Manifest-Objekt (hochgeladene manifest.json): Kernfelder weglassen,
   *  Handler als ganze Karte (Typen, die es nicht mehr gibt = null). */
  function manifestPatchFrom(fields) {
    const patch = {};
    for (const [k, v] of Object.entries(fields)) if (!SYSTEM_KEYS.has(k)) patch[k] = v;
    if (isObj(patch.handlers)) {
      const cur = effectiveManifest().handlers ?? {};
      patch.handlers = { ...Object.fromEntries(Object.keys(cur).filter((t) => !(t in patch.handlers)).map((t) => [t, null])), ...patch.handlers };
    }
    return patch;
  }
  /** Entwurfsfelder des Manifests aus dem Kern übernehmen (nach Kernaktionen, die das Manifest selbst ändern). */
  async function refreshManifestPatch() {
    const info = await src.draft.info();
    s.manifestPatch = structuredClone(info?.manifest ?? {});
  }
  const extrasOf = (m) => Object.fromEntries(Object.entries(m).filter(([k]) => !FORM_KEYS.includes(k) && !SYSTEM_KEYS.has(k)));
  function queueManifest(patch) {
    if (!Object.keys(patch).length) return;
    const prev = s.pendingManifest ?? {};
    s.pendingManifest = { ...prev, ...patch };
    // Handler-Patches zusammensetzen (beide gelten gegen dieselbe wirksame Karte des Kerns), nicht ersetzen
    if (isObj(prev.handlers) && isObj(patch.handlers)) s.pendingManifest.handlers = { ...prev.handlers, ...patch.handlers };
    clearTimeout(s.manifestTimer);
    s.manifestTimer = setTimeout(() => track(flushManifest()), WRITE_DEBOUNCE_MS);
    markTree(MANIFEST_VIEW);
    syncButtons();
  }
  async function flushManifest() {
    clearTimeout(s.manifestTimer);
    s.manifestTimer = null;
    const p = s.pendingManifest;
    s.pendingManifest = null;
    if (!p || !canEditSource(perms())) return;
    try {
      await src.draft.setManifest(p);
      await refreshManifestPatch(); // Kern-Ergebnis spiegeln (handlers wird dort gegen Basis + Entwurf gepatcht)
      if (!s.manifestInputError) sink.clear('source.manifest');
    } catch (e) {
      // Kern lehnt ab (z. B. Schutzfeld app_id → invalid): nichts übernommen; Eingabe bleibt im Formular.
      report('source.manifest', `Manifest nicht übernommen: ${errText(e)}`);
    }
    markTree(MANIFEST_VIEW);
    syncButtons();
  }

  // ---- Darstellung --------------------------------------------------------------------------------------------
  function groups() {
    const handlerPaths = new Set(Object.values(effectiveManifest().handlers ?? {}));
    const g = { handler: [], migration: [], files: [] };
    for (const p of livePaths()) {
      if (p === SCHEMA_FILE) continue;
      if (handlerPaths.has(p) || p.startsWith('handlers/')) g.handler.push(p);
      else if (p.startsWith('migrations/')) g.migration.push(p);
      else g.files.push(p);
    }
    return g;
  }
  const isFilePath = (p) => p !== MANIFEST_VIEW && p !== SCHEMA_FILE;
  function actButtons(path, anchor) {
    const ren = h('button', 'v4-src__act', '✎');
    ren.type = 'button';
    ren.dataset.rename = path;
    ren.title = 'Umbenennen';
    ren.setAttribute('aria-label', `${path} umbenennen`);
    ren.addEventListener('click', (e) => { e.stopPropagation(); startRename(path, anchor()); });
    const del = h('button', 'v4-src__act', '×');
    del.type = 'button';
    del.dataset.remove = path;
    del.title = 'Löschen';
    del.setAttribute('aria-label', `${path} löschen`);
    del.addEventListener('click', (e) => { e.stopPropagation(); removePath(path); });
    return [ren, del];
  }
  function treeRow(path, label) {
    const row = h('div', 'v4-src__row');
    const b = h('button', 'v4-src__item', label ?? path);
    b.type = 'button';
    b.dataset.path = path;
    b.title = path === MANIFEST_VIEW ? 'Manifest' : path;
    b.addEventListener('click', () => openPath(path));
    row.append(b);
    if (canEditSource(perms()) && isFilePath(path)) {
      const acts = h('span', 'v4-src__acts');
      acts.append(...actButtons(path, () => b));
      row.append(acts);
    }
    row.classList.toggle('is-current', s.current === path);
    b.classList.toggle('is-current', s.current === path);
    b.classList.toggle('is-changed', changed(path));
    return row;
  }
  function renderTree() {
    const g = groups();
    const sections = [
      ['App', [[MANIFEST_VIEW, 'Manifest'], ...(livePaths().includes(SCHEMA_FILE) ? [[SCHEMA_FILE, 'Collections']] : [])]],
      ['Handler', g.handler.map((p) => [p])],
      ['Migrationen', g.migration.map((p) => [p])],
      ['Dateien', g.files.map((p) => [p])],
    ];
    filesEl.replaceChildren();
    fileSelect.replaceChildren();
    for (const [name, items] of sections) {
      if (!items.length) continue;
      const sec = h('div', 'v4-src__group');
      sec.append(h('div', 'v4-src__group-title', name));
      const og = document.createElement('optgroup');
      og.label = name;
      for (const [p, label] of items) {
        sec.append(treeRow(p, label));
        const o = document.createElement('option');
        o.value = p;
        o.textContent = label ?? p;
        og.append(o);
      }
      filesEl.append(sec);
      fileSelect.append(og);
    }
    fileSelect.value = s.current ?? '';
    renderNarrowActs();
  }
  function renderNarrowActs() {
    narrowActs.replaceChildren();
    if (canEditSource(perms()) && s.current && isFilePath(s.current)) narrowActs.append(...actButtons(s.current, () => fileSelect));
  }
  function markTree(path) {
    const sel = path === MANIFEST_FILE ? MANIFEST_VIEW : path;
    let item = [...tree.querySelectorAll('.v4-src__item')].find((b) => b.dataset.path === sel);
    if (!item) { renderTree(); item = [...tree.querySelectorAll('.v4-src__item')].find((b) => b.dataset.path === sel); }
    item?.classList.toggle('is-changed', changed(sel));
  }
  function defaultPath() {
    const entry = effectiveManifest().entry;
    const paths = livePaths();
    return paths.includes(entry) ? entry : paths.find((p) => p.endsWith('.js')) ?? paths[0] ?? MANIFEST_VIEW;
  }

  let diffEl = null;
  async function openPath(path) {
    s.current = path;
    for (const r of tree.querySelectorAll('.v4-src__row')) {
      const cur = r.firstChild?.dataset.path === path;
      r.classList.toggle('is-current', cur);
      r.firstChild?.classList.toggle('is-current', cur);
    }
    fileSelect.value = path;
    renderNarrowActs();
    s.ed?.destroy();
    s.ed = null;
    diffEl = null;
    pane.replaceChildren();
    pane.dataset.view = path === MANIFEST_VIEW ? 'manifest' : path === SCHEMA_FILE ? 'collections' : 'file';
    el.dataset.currentPath = path;
    if (path === MANIFEST_VIEW) { renderManifest(); return; }
    if (s.diffMode) {
      // Diff-Umschalter: nur lesende Gegenüberstellung Basis ↔ Entwurf der aktuellen Datei.
      pane.dataset.view = 'diff';
      const host = h('div', 'v4-src__diffhost');
      host.dataset.diffPath = path;
      pane.append(host);
      renderTextDiff(host, s.basePaths.includes(path) ? baseTextOf(path) : '', s.draft.get(path) === null ? '' : textOf(path));
      return;
    }
    const host = h('div', 'v4-src__editor');
    pane.append(host);
    if (path === SCHEMA_FILE) { diffEl = h('div', 'v4-src__diff'); diffEl.dataset.v4SourceDiff = ''; pane.append(diffEl); renderDiff(); }
    const ed = await createEditor(host, {
      value: textOf(path), language: languageOf(path), readonly: !canEditSource(perms()), prefer, workerUrl: shellWorkerUrl(kernel),
      onChange: (v) => { setDraft(path, v); if (path === SCHEMA_FILE) previewDiff(); },
    });
    if (s.current !== path || s.closed) { ed.destroy(); return; }
    s.ed = ed;
    el.dataset.editor = ed.kind;
  }

  function renderDiff() {
    if (!diffEl) return;
    diffEl.replaceChildren();
    diffEl.dataset.source = s.diffSource;
    if (s.schemaError) return; // Fehler steht in der Zustandszone
    const edit = canEditSource(perms());
    for (const d of s.diff) {
      const row = h('div', 'v4-src__coll');
      row.dataset.collection = d.name;
      row.dataset.kind = d.kind;
      const head = h('div', 'v4-src__coll-head');
      head.append(h('span', 'v4-src__coll-name', d.name), h('span', 'v4-src__badge', KIND_LABEL[d.kind]));
      row.append(head);
      if (d.reasons.length && d.kind !== 'new') row.append(h('div', 'v4-src__coll-reasons', d.reasons.join(' · ')));
      if (d.needsMigration) {
        const mig = h('div', 'v4-src__coll-mig');
        const blocker = s.blockers.find((b) => b.name === d.name);
        if (!blocker && d.migration?.migrate_fn) {
          const open = h('button', 'v4-src__link', d.migration.migrate_fn);
          open.type = 'button';
          open.dataset.openMigration = d.name;
          open.addEventListener('click', () => openPath(d.migration.migrate_fn));
          mig.append(h('span', null, 'Migration '), open);
        } else if (!blocker) {
          mig.append(h('span', null, `Migration: ${(d.migration?.steps ?? []).map((x) => x.op).join(', ')}`));
        } else {
          mig.dataset.missing = '';
          mig.append(h('span', null, 'Migration nötig '));
          if (edit) {
            const add = h('button', 'v4-src__btn v4-src__btn--small', 'Migration anlegen');
            add.type = 'button';
            add.dataset.addMigration = d.name;
            add.addEventListener('click', () => addMigration(d));
            mig.append(add);
          }
        }
        row.append(mig);
      }
      diffEl.append(row);
    }
  }

  async function addMigration(d) {
    const path = d.migration?.migrate_fn ?? migrationPath(d.name);
    if (!livePaths().includes(path)) setDraft(path, migrationTemplate(d.name, s.base, d.reasons));
    const nextSchema = withMigration(textOf(SCHEMA_FILE), d.name, s.base, path);
    setDraft(SCHEMA_FILE, nextSchema);
    if (s.current === SCHEMA_FILE) s.ed?.setValue(nextSchema);
    renderTree();
    await flush(); // Migrationsdatei vor dem Schema schreiben lassen, dann liefert der Kern den neuen Diff
  }

  function renderManifest() {
    const m = effectiveManifest();
    const form = h('form', 'v4-src__form');
    form.dataset.v4SourceManifest = '';
    const edit = canEditSource(perms());
    const field = (key, label, value, { area = false, rows = 4 } = {}) => {
      const row = h('label', 'v4-src__field');
      row.append(h('span', 'v4-src__field-label', label));
      const inp = document.createElement(area ? 'textarea' : 'input');
      inp.name = key;
      inp.value = value;
      inp.readOnly = !edit;
      inp.spellcheck = false;
      if (area) inp.rows = rows;
      row.append(inp);
      form.append(row);
      return inp;
    };
    field('title', 'Titel', m.title ?? '');
    field('entry', 'Einstieg', m.entry ?? 'index.js');
    field('min_app_api', 'App-API', m.min_app_api ?? '4.0');
    field('capabilities_required', 'Fähigkeiten', (m.capabilities_required ?? []).join(', '));
    field('handlers', 'Handler', Object.entries(m.handlers ?? {}).map(([t, p]) => `${t} = ${p}`).join('\n'), { area: true });
    const extras = extrasOf(m);
    field('extras', 'Weitere Felder', Object.keys(extras).length ? JSON.stringify(extras, null, 2) : '', { area: true, rows: 5 })
      .placeholder = '{ "triggers": […] }';
    form.addEventListener('submit', (e) => e.preventDefault());
    form.addEventListener('input', () => {
      const fd = new FormData(form);
      const desired = {
        title: String(fd.get('title') ?? '').trim(),
        entry: String(fd.get('entry') ?? '').trim() || 'index.js',
        min_app_api: String(fd.get('min_app_api') ?? '').trim() || '4.0',
        capabilities_required: String(fd.get('capabilities_required') ?? '').split(',').map((x) => x.trim()).filter(Boolean),
        handlers: Object.fromEntries(String(fd.get('handlers') ?? '').split('\n').map((l) => l.split('=').map((x) => x.trim())).filter(([t, p]) => t && p)),
      };
      const eff = effectiveManifest();
      const patch = {};
      for (const k of FORM_KEYS) {
        if (same(eff[k], desired[k])) continue;
        // handlers als Patch gegen die wirksame Karte (Kern: Basis + Entwurf); entfernte Typen = null, auch Basis-Handler
        patch[k] = k === 'handlers'
          ? { ...Object.fromEntries(Object.keys(eff.handlers ?? {}).filter((t) => !(t in desired.handlers)).map((t) => [t, null])), ...desired.handlers }
          : desired[k];
      }
      const raw = String(fd.get('extras') ?? '').trim();
      let next = {};
      try { next = raw ? JSON.parse(raw) : {}; if (!isObj(next)) throw new Error('JSON-Objekt erwartet'); s.manifestInputError = null; } catch (e) {
        next = null;
        s.manifestInputError = errText(e);
        report('source.manifest', `Weitere Manifest-Felder: ${s.manifestInputError}`);
      }
      if (next) {
        const prev = extrasOf(eff);
        for (const [k, v] of Object.entries(next)) if (!same(prev[k], v)) patch[k] = v;
        for (const k of Object.keys(prev)) if (!(k in next)) patch[k] = null;
        if (!Object.keys(patch).length) sink.clear('source.manifest');
      }
      queueManifest(patch);
    });
    pane.append(form);
  }

  function syncButtons() {
    const p = perms();
    const edit = canEditSource(p);
    const active = activeOf();
    el.dataset.stale = s.base !== null && active !== null && active !== s.base ? '1' : '';
    el.classList.toggle('is-readonly', !edit);
    uploadBtn.hidden = !edit;
    diffBtn.setAttribute('aria-pressed', s.diffMode ? 'true' : 'false');
    diffBtn.classList.toggle('is-on', s.diffMode);
    agentItem.hidden = !agent;
    if (agent) agentItem.textContent = agent.open ? 'Agent ausblenden' : 'Agent einblenden';
    discardBtn.hidden = !edit || !hasChanges();
    publishBtn.hidden = !edit;
    publishBtn.disabled = s.publishing || !hasChanges() || s.blockers.length > 0;
    publishBtn.title = s.schemaError ? `${SCHEMA_FILE} ist ungültig` : s.blockers.length ? `Migration fehlt: ${s.blockers.map((b) => b.name).join(', ')}` : '';
    s.ed?.setReadonly(!edit);
  }

  // ---- Dateien: Löschen / Umbenennen (A7) ----------------------------------------------------------------------
  function startRename(path, anchor) {
    if (!anchor || anchor.hidden) return;
    const input = h('input', 'v4-src__input v4-src__rename');
    input.value = path;
    input.spellcheck = false;
    input.dataset.renameInput = path;
    input.setAttribute('aria-label', `Neuer Pfad für ${path}`);
    anchor.hidden = true;
    anchor.after(input);
    input.focus();
    input.setSelectionRange(path.lastIndexOf('/') + 1, path.length);
    let done = false;
    const end = (commit) => {
      if (done) return;
      done = true;
      const to = input.value.trim();
      input.remove();
      anchor.hidden = false;
      if (commit && to && to !== path) renamePath(path, to);
    };
    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') { e.preventDefault(); end(true); } else if (e.key === 'Escape') { e.preventDefault(); end(false); }
    });
    input.addEventListener('blur', () => end(true));
  }
  async function renamePath(from, to) {
    await writeNow(from);
    await Promise.all([...s.writes]);
    const text = textOf(from);
    try {
      await src.draft.rename(from, to);
    } catch (e) {
      report('source.file', `„${from}“ nicht umbenannt: ${errText(e)}`);
      return;
    }
    sink.clear('source.file');
    s.draft.set(to, text);
    s.draft.set(from, null);
    // Verweise im Manifest (entry, handlers, styles) zieht der Kern mit (§16.26) – hier nur dessen Ergebnis übernehmen.
    await refreshManifestPatch();
    s.blockers = s.schemaError ? s.blockers : publishBlockers(s.diff, livePaths());
    renderTree();
    if (s.current === from) await openPath(to);
    else if (s.current === MANIFEST_VIEW) await openPath(MANIFEST_VIEW); // Formular zeigt die mitgezogenen Verweise
    syncButtons();
  }
  async function removePath(path) {
    await writeNow(path);
    await Promise.all([...s.writes]);
    try {
      await src.draft.remove(path);
    } catch (e) {
      report('source.file', `„${path}“ nicht gelöscht: ${errText(e)}`);
      return;
    }
    sink.clear('source.file');
    s.draft.set(path, null);
    s.blockers = s.schemaError ? s.blockers : publishBlockers(s.diff, livePaths());
    renderTree();
    if (s.current === path) await openPath(defaultPath());
    renderDiff();
    syncButtons();
  }

  // ---- Aktionen ------------------------------------------------------------------------------------------------
  async function publish() {
    if (s.publishing) return;
    s.publishing = true;
    syncButtons();
    try {
      await flush();
      if (s.blockers.length) return;
      // Strang LOOP: verknüpfte offene Meldungen → Versionsnotiz „behebt Meldung #N“ (im Fensterblatt änderbar);
      // Abbrechen im Blatt veröffentlicht nicht.
      const linked = reportsTab ? reportsTab.linked(agent?.state?.draftId ?? null) : [];
      let note = '';
      if (linked.length) {
        const all = reportsTab.rows();
        const proposal = fixNoteFor(linked.map((x) => reportNo(x, all)));
        note = sheet?.dialog ? await sheet.dialog({ kind: 'prompt', title: 'Veröffentlichen', text: 'Versionsnotiz', value: proposal, multiline: true, confirm_label: 'Veröffentlichen' }) : proposal;
        if (note == null) return;
        note = String(note).trim();
      }
      const r = await src.draft.publish({ base_app_ver: s.base }).result;
      if (r.status === 'accepted') {
        sink.clear('source.conflict');
        sink.clear('source.publish');
        if (linked.length) {
          const all = reportsTab.rows();
          const named = notedNos(note);
          const fixed = linked.filter((x) => named.has(reportNo(x, all)));
          if (fixed.length) {
            try { await markReportsFixed(kernel.shellAppApi, fixed, { ver: r.rev, note, actorId: kernel.shellAppApi.actor?.id ?? '' }); sink.clear('source.reports'); }
            catch (e) { report('source.reports', `Meldungen nicht als behoben gespeichert: ${errText(e)}`); }
          }
        }
        await reload(r.rev);
      } else if (r.reason === 'stale_base') {
        const active = activeOf();
        report('source.conflict', `Veröffentlichen abgelehnt: Basis ${s.base ?? '–'} ist nicht mehr aktiv${active ? ` (aktiv: ${active})` : ''}. Dein Entwurf bleibt erhalten.`,
          { action: { label: 'Auf aktuelle Version setzen', run: () => rebase() } });
      } else if (r.reason === 'busy') {
        report('source.conflict', 'Veröffentlichen abgelehnt: eine andere Veröffentlichung dieser App ist noch offen. Dein Entwurf bleibt erhalten.');
      } else {
        // z. B. invalid „Verweis auf fehlende Datei: handlers.<typ> → <pfad>“ oder „Migration fehlt: <coll>“ (§16.26)
        report('source.publish', `Veröffentlichen abgelehnt (${r.reason})${r.detail ? `: ${r.detail}` : ''}. Dein Entwurf bleibt erhalten.`);
      }
    } finally {
      s.publishing = false;
      syncButtons();
    }
  }
  async function rebase() {
    const active = activeOf();
    await flush();
    try {
      await src.draft.rebase(active);
    } catch (e) {
      report('source.conflict', `Basis nicht umgesetzt: ${errText(e)}`);
      return;
    }
    sink.clear('source.conflict');
    await reload(active);
  }
  async function discard() {
    for (const t of s.timers.values()) clearTimeout(t);
    s.timers.clear();
    clearTimeout(s.manifestTimer);
    s.manifestTimer = null;
    s.pendingManifest = null;
    await Promise.all([...s.writes]);
    try {
      await src.draft.discard();
    } catch (e) {
      report('source.draft', `Entwurf nicht verworfen: ${errText(e)}`);
      return;
    }
    for (const c of ['source.conflict', 'source.publish', 'source.schema', 'source.manifest', 'source.file']) sink.clear(c);
    s.schemaError = null;
    s.manifestInputError = null;
    await reload(activeOf());
  }
  publishBtn.addEventListener('click', publish);
  discardBtn.addEventListener('click', discard);
  uploadBtn.addEventListener('click', () => { closeMore(); folderInput.click(); });
  agentItem.addEventListener('click', () => { closeMore(); agent?.setOpen(!agent.open); syncButtons(); });
  diffBtn.addEventListener('click', () => setDiffMode(!s.diffMode));
  function closeMore() { moreMenu.hidden = true; moreBtn.setAttribute('aria-expanded', 'false'); }
  moreBtn.addEventListener('click', () => {
    const open = moreMenu.hidden;
    moreMenu.hidden = !open;
    moreBtn.setAttribute('aria-expanded', String(open));
    if (open) moreMenu.querySelector('.v4-src__menu-item:not([hidden])')?.focus();
  });
  moreMenu.addEventListener('keydown', (e) => {
    const items = [...moreMenu.querySelectorAll('.v4-src__menu-item:not([hidden])')];
    const i = items.indexOf(document.activeElement);
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closeMore(); moreBtn.focus(); }
    else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); items[(i + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length]?.focus(); }
  });
  const onDocDown = (e) => { if (!moreWrap.contains(e.target)) closeMore(); };
  document.addEventListener('pointerdown', onDocDown, true);
  async function setDiffMode(on) {
    await flush();
    s.diffMode = !!on;
    syncButtons();
    if (s.current) await openPath(s.current);
  }
  /** Ausstehende Eingaben schreiben und auf die Core-Bestätigung warten. → {rev} der zuletzt bestätigten Schreibung,
   *  {rejected} bei Ablehnung, null wenn nichts geschrieben wurde oder der Kern kein result liefert (alte Kerne). */
  async function flushConfirmed() {
    await flush();
    const pending = s.writeResults.splice(0);
    if (!pending.length) return null;
    const last = (await Promise.all(pending)).at(-1);
    if (last?.status === 'accepted' && last.rev) return { rev: last.rev };
    if (last?.status === 'rejected') return { rejected: last };
    return null;
  }
  /** Geänderte Dateien als Diff öffnen (nach einem Agent-Turn oder übernommenem Patch). */
  async function openChanges(paths = []) {
    const live = livePaths();
    const target = paths.find((p) => live.includes(p)) ?? paths[0];
    s.diffMode = true;
    renderTree();
    syncButtons();
    if (target) await openPath(target);
  }

  // ---- Coding Agent (Seitenleiste rechts, §16.44) ----------------------------------------------------------------
  // Port über die App-API der Shell (§16.44: business_commands + coding_turns). Ohne sie keine Seitenleiste.
  // (Name shellAppApi, nicht shellApi: der Stub-Kernel hat eine gleichnamige Funktion kernel.shellApi().)
  const coding = typeof kernel.shellAppApi?.query === 'function' && typeof kernel.shellAppApi?.command === 'function'
    ? createCodingPort(kernel.shellAppApi, kernel.source) : null;
  if (coding) {
    agent = createAgentPanel({
      appId, appTitle: appRecord.title || appId, coding, source: src, zone: sink,
      canEdit: () => canEditSource(perms()),
      activeVer: () => activeOf(),
      flushDraft: () => flushConfirmed(),
      reloadDraft: () => reload(s.base ?? activeOf()),
      openChanges: (paths) => openChanges(paths),
    });
    main.append(agent.el, agent.tab);
    el.classList.add('has-agent-panel');
    agent.setOpen(agentOpen);
  }
  fileSelect.addEventListener('change', () => openPath(fileSelect.value));

  // ---- Meldungen (Strang LOOP) ---------------------------------------------------------------------------------
  const reportsApi = kernel.shellAppApi;
  let reportsTab = null;
  function showSide(which) {
    const rep = which === 'reports' && !!reportsTab;
    filesEl.hidden = rep;
    if (reportsTab) reportsTab.el.hidden = !rep;
    filesTabBtn.classList.toggle('is-on', !rep);
    reportsTabBtn.classList.toggle('is-on', rep);
    filesTabBtn.setAttribute('aria-selected', String(!rep));
    reportsTabBtn.setAttribute('aria-selected', String(rep));
    el.dataset.side = rep ? 'reports' : 'files';
  }
  async function assignReport(rep, no) {
    if (!agent) { report('source.reports', 'Der Coding Agent ist hier nicht verfügbar.'); return null; } // v4-allow: zone
    const prompt = codingPromptFor(rep, { no, appTitle: appRecord.title || appId, appVer: activeOf() ?? '', zoneErrors: recentZoneErrors(sink) });
    agent.setOpen(true);
    syncButtons();
    const r = await agent.send(prompt, { record_id: rep.id });
    if (r?.status !== 'running' && r?.status !== 'completed') return r; // Ablehnung steht schon in der Zone (agent.turn)
    try {
      await linkReportToTurn(reportsApi, rep, { turn_id: r.turn_id, command_id: r.command_id ?? '', draft_id: r.draft_id ?? '' });
      sink.clear('source.reports');
    } catch (e) {
      report('source.reports', `Meldung #${no} nicht mit dem Agent-Lauf verknüpft: ${errText(e)}`);
    }
    return r;
  }
  if (reportsApi && typeof reportsApi.patch === 'function') {
    reportsTab = createReportsTab({
      appId, api: reportsApi, canEdit: () => canEditSource(perms()) && !!agent,
      onAssign: (rep, no) => assignReport(rep, no),
      onOpenTurn: (turnId) => { agent?.focusTurn(turnId); syncButtons(); },
    });
    reportsTab.el.hidden = true;
    tree.append(reportsTab.el);
    sideTabs.hidden = false;
    reportsTab.onCount((n) => { reportsCount.textContent = n ? ` ${n}` : ''; reportsTabBtn.dataset.count = String(n); });
    filesTabBtn.addEventListener('click', () => showSide('files'));
    reportsTabBtn.addEventListener('click', () => showSide('reports'));
    showSide('files');
  }
  // api.shell.openSource(appId, {tab, reportId}): Reiter wählen, Meldung markieren (reports-tab.js mark).
  function applyFocus(f) {
    if (!f || s.closed) return;
    showSide(f.tab === 'files' ? 'files' : 'reports');
    if (f.tab !== 'files') reportsTab?.mark(f.reportId ?? null);
  }
  applyFocus(focus);

  // ---- Start, Live-Folgen --------------------------------------------------------------------------------------
  await reload(activeOf() ?? appRecord.active_ver ?? null);

  // Neue aktive Version ohne eigenen Entwurf → still folgen (der Editor bleibt offen). Mit Entwurf: Basis bleibt;
  // Veröffentlichen würde mit stale_base abgelehnt und bietet dann „Auf aktuelle Version setzen“ an.
  let lastActive = activeOf();
  const offApps = kernel.apps.subscribe(() => {
    const a = activeOf();
    if (a === lastActive || s.closed) return;
    lastActive = a;
    if (!hasChanges() && !s.publishing && a && a !== s.base) reload(a);
    else syncButtons();
  });
  const offPerms = api.actor.permissions.subscribe((p) => {
    if (s.closed) return;
    if (!canViewSource(p)) { handle.close(); return; }
    syncButtons();
    renderTree();
    if (diffEl) renderDiff();
  });

  const handle = {
    el,
    close() {
      if (s.closed) return;
      s.closed = true;
      for (const path of [...s.timers.keys()]) writeNow(path); // Entwurf nicht verlieren
      if (s.manifestTimer) flushManifest();
      offApps?.();
      offPerms?.();
      agent?.destroy();
      reportsTab?.destroy();
      document.removeEventListener('pointerdown', onDocDown, true);
      s.ed?.destroy();
      zoneGuard.detach();
      el.remove();
    },
    // Für Tests/Integration: Zustand lesen, ohne DOM zu parsen.
    get state() {
      return {
        base: s.base, draft: [...s.draft.keys()], deleted: [...s.draft].filter(([, t]) => t === null).map(([p]) => p),
        paths: livePaths(), manifest: structuredClone(s.manifestPatch), blockers: s.blockers.map((b) => ({ ...b })),
        diff: s.diff.map((d) => ({ name: d.name, kind: d.kind })), diffSource: s.diffSource, deviations: structuredClone(s.deviations),
      };
    },
    flush,
    get editor() { return s.ed; },
    get agent() { return agent; },
    get reports() { return reportsTab; },
    showSide,
    focus: applyFocus,
    assignReport,
    openChanges,
    setDiffMode,
    get diffMode() { return s.diffMode; },
  };
  return handle;
}
