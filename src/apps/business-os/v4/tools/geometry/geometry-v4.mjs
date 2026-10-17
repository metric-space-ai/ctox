#!/usr/bin/env node
// Shell v4 – Geometrie-Labor (Strang GEO).
//
// Rendert jede portierte App im echten v4-Rahmen (v4/frame/index.js + v4/shell/app-host.js) gegen den Kern-Stub
// (Pin 8265bd7, abweichend per CTOX_APP_API_STUB) mit Fixture-Daten aus collections.schema.json (fixtures.mjs),
// misst je Breite G1–G6 und legt Screenshots ab.
//
//   node v4/tools/geometry/geometry-v4.mjs [--app <id>[,<id>…]] [--widths 1280,900,420] [--out <dir>]
//
// Portiert = index.js exportiert mount(<root>, <api> …) (nicht ctx) UND `scan.mjs --app <id>` ≤ 1 Stelle
// (nur ein schema.js-Rest erlaubt). Alle anderen werden übersprungen und aufgelistet.
//
// Kriterien je App und Breite (Fenster maximiert im Rahmen, Viewport = Breite):
//   G1 Inhalt bleibt im Inhaltsbereich: kein sichtbarer Teil eines Elements links/rechts/oberhalb von root, kein
//      horizontales Scrollen (root und innere Scroll-Container), nichts an document.body, keine App-Pixel über
//      Fensterkopf, Zone oder Taskleiste (Punktabtastung elementFromPoint).
//   G2 Titel über api.shell.setTitle: die App ruft setTitle selbst (das Labor veröffentlicht OHNE Titel, der Host
//      setzt vorab nur die App-ID), Endtitel nicht leer, keine App-ID, keine UUID, keine Fixture-Zeilen-ID.
//   G3 keine sichtbaren Zustandstexte im Inhalt (v4/tools/guards/vocabulary.json; ausgenommen Elemente unter
//      [data-v4-allow…] und Literale aus Quellzeilen mit `v4-allow: label`), Zustandszone ohne Einträge.
//   G4 keine abgeschnittenen Texte (text-overflow: ellipsis bzw. line-clamp mit Überlauf), Ausnahme
//      [data-v4-allow-ellipsis].
//   G5 App-CSS wirkt nicht außerhalb des App-Inhalts: berechnete Stile aller Elemente außerhalb von root mit und
//      ohne die App-Stylesheets verglichen (entscheidend); Selektor-Treffer außerhalb als Beleg.
//   G7 Geschwister-Pane-Köpfe gleich hoch: sichtbare .ctox-pane-header mit gleicher Oberkante (Spalten nebeneinander)
//      sind gleich hoch (±1 px); ein langer Titel darf einen Kopf wachsen lassen, die Geschwister wachsen mit
//      (v4/frame/pane-heads.js). Strang VIS4.
//   Zonen-Fälle (zone-cases.mjs, nur 1280): Fixture gezielt im Fehlerzustand, Zone mit erwartetem Code muss sichtbar
//      sein, Inhalt ohne Zustandstexte. Eigene Liste results.zoneCases, nicht Teil der App×Breite-Zählung.
//   G8 Reservierte Zonen (FR6a): kein interaktives Element der App schneidet die Rechtecke unter dem schwebenden
//      Icon-Header (oben links) und dem Knopfcluster (oben rechts); Maße aus den Zonen-Variablen des Rahmens.
//   G9 Kantenpalette aus den Icon-Pixeln berechnet (shellV2PaletteSource 'icon' | 'generated', nie leer) UND nahtlos
//      im Screenshot (g9.mjs, Strang PAL): ΔE zwischen DECKENDEM Logopixel am Rand (nächster Pixel mit Alpha ≥ 250 zur
//      oberen rechten bzw. unteren linken Icon-Ecke) und der Kante daneben < 12. Nicht mehr der Eckpixel: bei Icons mit
//      durchsichtigen Ecken zeigt der den Halter-Hintergrund und verdeckte die Naht (M-17). Gegenprobe g9.test.mjs.
//   G6 Layout folgt der Fensterbreite, nicht dem Viewport: Layout-Signatur (Spaltenzahl + sichtbare Bausteine) der
//      maximierten App im Viewport B gleich der eines freien, gleich breiten Fensters im Viewport 1280; dazu statisch
//      keine @media mit Breitenbedingung in den App-Stylesheets.
//   G11 schmal (nur Breiten < 600, Strang KIT1, g11.mjs): höchstens eine Pane-Spalte sichtbar, Reiter einzeilig, kein
//      horizontaler Überlauf; Detail per echtem Tipp auf eine Listenzeile erreichbar, „Zurück“ außerhalb der Zonen
//      führt zur Liste mit wiederhergestelltem Scrollstand (pane-narrow.js).
//
// Ausgaben: <out>/<app>/<breite>.png, <out>/<app>/g6-<breite>-in-1280.png, <out>/results.json, <out>/index.html.
import { createServer } from 'node:http';
import { readFileSync, existsSync, statSync, readdirSync, mkdirSync, writeFileSync } from 'node:fs';
import { join, dirname, extname, normalize, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { buildFixtures } from './fixtures.mjs';
import { measureInPage, sameLayout } from './measure.mjs';
import { g11Static, g11Probe } from './g11.mjs';
import { g9Pixel } from './g9.mjs';
import { ZONE_CASES, judgeZoneCase } from './zone-cases.mjs';
import vm from 'node:vm';
import { pathToFileURL } from 'node:url';
import { scanApp } from '../port/scan.mjs';
import { realManifestRoutes } from '../port/real-manifest.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '..', '..', '..'));
const MODULES = join(BOS, 'modules');
const MANIFESTS = join(BOS, 'v4', 'manifests');
const STUB = normalize(process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/64acc2c'));
const VOCAB = JSON.parse(readFileSync(join(BOS, 'v4', 'tools', 'guards', 'vocabulary.json'), 'utf8')).entries;

const args = process.argv.slice(2);
const opt = (name, dflt) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : dflt; };
const WIDTHS = opt('--widths', '1280,900,420').split(',').map(Number).filter(Boolean);
const OUT = normalize(opt('--out', join(HERE, 'out', args.includes('--shared') ? 'shared-css' : 'frame')));
const ONLY = opt('--app', '') ? opt('--app').split(',') : null;
const SHARED = args.includes('--shared');
const BASE_W = 1280;
const HEIGHT = { 1280: 800, 900: 800, 420: 860 };
const vh = (w) => HEIGHT[w] || (w <= 480 ? 860 : 800);

// Playwright aus dem eigenen Paket (src/apps/business-os), nicht aus einem festen Checkout – sonst findet die CI es nicht.
const require = createRequire(join(HERE, '../../../package.json'));
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const EXE = join(homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');

// ---------------------------------------------------------------------------------------------------------------
// Auswahl der portierten Apps
// ---------------------------------------------------------------------------------------------------------------
const MOUNT_RE = /export\s+(?:async\s+)?function\s+mount\s*\(\s*([A-Za-z_$][\w$]*)\s*,\s*([A-Za-z_$][\w$]*)/;
function classify(id) {
  const dir = join(MODULES, id);
  const js = join(dir, 'index.js');
  if (!existsSync(js)) return { id, ported: false, reason: 'kein index.js' };
  const m = MOUNT_RE.exec(readFileSync(js, 'utf8'));
  const scan = scanApp(dir, id).findings.filter((f) => f.rule !== 'R16-css');
  const rest = scan.map((f) => `${f.rule}:${f.file}${f.line ? `:${f.line}` : ''}`);
  if (!m || m[1] === 'ctx') return { id, ported: false, reason: `kein mount(root, api) (${m ? `mount(${m[1]}, …)` : 'kein mount-Export'}); Scan ${scan.length}`, rest };
  if (scan.length > 1) return { id, ported: false, reason: `Scan ${scan.length} Stellen (> 1)`, rest };
  if (scan.length === 1 && scan[0].rule !== 'R17-schema') return { id, ported: false, reason: `Scan-Rest ${rest[0]} ist kein schema.js-Rest`, rest };
  return { id, ported: true, rest };
}
const ALL = readdirSync(MODULES).filter((n) => statSync(join(MODULES, n)).isDirectory() && existsSync(join(MODULES, n, 'module.json'))).sort();
const IS_MAIN = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
// Nur beim direkten Aufruf klassifizieren (der Import durch geometry-v4.test.mjs braucht nur Server und Umschreibung).
const classes = IS_MAIN ? ALL.map(classify) : [];
const PORTED = classes.filter((c) => c.ported && (!ONLY || ONLY.includes(c.id)));
const SKIPPED = classes.filter((c) => !c.ported);

// ---------------------------------------------------------------------------------------------------------------
// Je App: Konfiguration für die Laborseite
// ---------------------------------------------------------------------------------------------------------------
function readJson(f) { try { return JSON.parse(readFileSync(f, 'utf8')); } catch { return null; } }
function sourceFiles(dir, out = []) {
  for (const n of readdirSync(dir)) {
    const p = join(dir, n);
    if (statSync(p).isDirectory()) { if (!/^(tests?|node_modules|handlers|qa|dist)$/.test(n)) sourceFiles(p, out); }
    else if (/\.(m?js|html)$/.test(n) && !/\.test\.|test\.mjs$|smoke|e2e/.test(n)) out.push(p);
  }
  return out;
}
// Literale aus Zeilen mit `v4-allow: label` (gleiche oder folgende Zeile, wie guard-v4.mjs).
function allowedLabels(id) {
  const out = new Set();
  for (const f of sourceFiles(join(MODULES, id))) {
    const lines = readFileSync(f, 'utf8').split('\n');
    lines.forEach((line, i) => {
      if (!/v4-allow:\s*label/.test(line)) return;
      for (const l of [line, lines[i + 1] || '']) {
        for (const m of l.matchAll(/'([^'\n]{2,120})'|"([^"\n]{2,120})"|`([^`\n]{2,120})`|>([^<>\n]{2,120})</g)) {
          const t = (m[1] || m[2] || m[3] || m[4] || '').replace(/\$\{[^}]*\}/g, ' ').trim();
          if (t && /\p{L}{2}/u.test(t) && !/v4-allow/.test(t)) out.add(t);
        }
      }
    });
  }
  return [...out];
}
function staticCss(id) {
  const media = [], containers = [];
  for (const f of readdirSync(join(MODULES, id)).filter((n) => n.endsWith('.css'))) {
    const css = readFileSync(join(MODULES, id, f), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');
    for (const m of css.matchAll(/@media[^{]*?\((?:max|min)-width\s*:\s*(\d+)px/g)) media.push(`${f}: @media ${m[1]}px`);
    for (const m of css.matchAll(/@container[^{]*?\((?:max|min)-width\s*:\s*(\d+)px|@container[^{]*?width\s*[<>]=?\s*(\d+)px/g)) containers.push(Number(m[1] || m[2]));
  }
  return { media, containers: [...new Set(containers)].sort((a, b) => b - a) };
}
export function appConfig(id) {
  const manifest = readJson(join(MANIFESTS, `${id}.manifest.json`)) || {};
  const ml = manifest.layout || {};
  const layout = {};
  for (const k of ['icon_svg', 'icon_asset', 'category', 'default_width', 'default_height', 'min_width', 'min_height']) if (ml[k] != null) layout[k] = ml[k];
  if (!layout.icon_svg && !layout.icon_asset && existsSync(join(MODULES, id, 'icon.svg'))) layout.icon_asset = 'icon.svg';
  const styles = Array.isArray(manifest.styles) ? manifest.styles : (existsSync(join(MODULES, id, 'index.css')) ? ['index.css'] : []);
  const { seed, skipped } = buildFixtures(id);
  // E48 (Stub 8265bd7): eigene und Fremd-Collections lesen nur mit veröffentlichtem echtem Manifest (Routen, Rechte,
  // collections, foreign_collections) und aktiven Eigentümern (_fixture_owners, inkl. ctox-platform).
  const routes = existsSync(join(MANIFESTS, `${id}.manifest.json`)) ? realManifestRoutes(id) : {};
  // Die Shell liest shell_preferences (ctox-platform) über appApi('shell', …): ebenfalls echtes Shell-Manifest.
  const shellRoutes = realManifestRoutes('shell');
  return { id, entry: manifest.entry || 'index.js', layout, styles, seed, routes, shellRoutes, skippedCollections: skipped, allow: allowedLabels(id) };
}

// ---------------------------------------------------------------------------------------------------------------
// Server: Wurzel = business-os; /~a/<app>@<ver>/<pfad> → modules/<app>/<pfad> (sonst business-os/<pfad>),
// /~stub/ → Kern-Stub, /__geo/ → Laborseite. Umschreiben wie der Service Worker (v4/sw.js, §16.15): dessen eigene
// rewriteShellRefs (per vm geladen, keine Kopie) mit Shell-Version LAB_SHELL; ctox:shared/ in JS und CSS →
// /~a/shell@lab/shared/… (→ business-os/shared/…). ctox:lib/<name>@<major>/ schreibt sw.js nach Kern §16.37/§16.39
// auf /~a/lib:<name>@<major>.<minor>[~lib:…]/… um – mit der Bindung aus der URL-Wurzel der anfragenden Datei
// (parseBinding aus sw.js). Die Bindung baut der Kern-Stub (artifacts.bind je Mount, K-1d): die Laborseite
// veröffentlicht dazu die Shell LAB_SHELL mit libs aller freigegebenen Bibliotheken und je Bibliothek
// lib:<name>@<major>.0 (LAB_LIB_BINDING); App-URLs tragen damit ~shell@lab~lib:…. Ungebundene URLs bekommen keine
// Ersatzbindung (L-34 erledigt). Bibliotheks-URLs löst er aus dem Quellordner der Bibliothek auf (v4/tools/isolation/standard-libs.json,
// nur status "approved", sonst 404 wie im SW); die Minor spielt dort keine Rolle. Alte Formen (/~a/shell@<v>/lib/…,
// /~a/lib:<name>@<major>/… ohne Minor) gibt es nicht mehr.
// ---------------------------------------------------------------------------------------------------------------
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.jpg': 'image/jpeg', '.woff2': 'font/woff2', '.woff': 'font/woff', '.ttf': 'font/ttf', '.wasm': 'application/wasm', '.map': 'application/json', '.txt': 'text/plain' };
export const configs = new Map();
export const LAB_SHELL = 'lab';
// sw.js ist ein klassisches Worker-Skript: in einem vm-Kontext mit Attrappe für self laden (wie ctox-lib.test.mjs).
export function loadSwRewrite(swFile = join(BOS, 'v4', 'sw.js')) {
  const ctx = { self: { addEventListener() {}, location: { origin: 'http://geo' }, clients: {} }, URL, TextDecoder, Response: class {} };
  vm.createContext(ctx);
  vm.runInContext(`${readFileSync(swFile, 'utf8')}\n;globalThis.__api = { rewriteShellRefs, rewriteLibRefs, parseBinding, STANDARD_LIBS };`, ctx);
  return ctx.__api;
}
const SW = loadSwRewrite();
// Freigegebene Standardbibliotheken: <name>@<major> → Quellordner (erstes Wort von "source", z. B. vendor/ctox-office/).
export const LIB_DIRS = Object.fromEntries(Object.entries(readJson(join(BOS, 'v4', 'tools', 'isolation', 'standard-libs.json'))?.libs || {})
  .filter(([k, v]) => v.status === 'approved' && SW.STANDARD_LIBS.includes(k))
  .map(([k, v]) => [k, normalize(join(BOS, String(v.source || '').split(/\s+/)[0]))]));
// Bindung, die der Labor-Kernel je Mount liefert (veröffentlichte lib:<name>@<major>.0): {"<name>@<major>": "<major>.0"}.
export const LAB_LIB_BINDING = Object.freeze(Object.fromEntries(Object.keys(LIB_DIRS).map((k) => [k, `${k.split('@')[1]}.0`])));
// Bindung der anfragenden URL wie im SW: Wurzel "/~a/<wurzel>/…"; Bibliotheks-URL bindet sich selbst (Kopf) plus Rest.
// Ungebundene App-URL → {} (kein Ersatz). Mehrdeutig → null.
export function labBinding(p) {
  const root = /^\/~a\/([^/]+)\//.exec(p)?.[1] || '';
  const head = /^lib:([a-z0-9][a-z0-9._-]*)@(\d+)\.(\d+)/.exec(root);
  const rest = SW.parseBinding(head ? root.slice(head[0].length) : root);
  if (!rest) return null;
  const binding = { ...rest };
  if (head) {
    const key = `${head[1]}@${head[2]}`;
    if (key in binding) return null;
    binding[key] = `${head[2]}.${head[3]}`;
    return binding;
  }
  return binding;
}
// Pfad → Datei (oder null). Exportiert für geometry-v4.test.mjs.
export function resolveLabPath(p) {
  const a = /^\/~a\/([^/@]+)@([^/]+)\/(.+)$/.exec(p);
  const st = /^\/~stub\/(.+)$/.exec(p);
  let file;
  // Form §16.39: /~a/lib:<name>@<major>.<minor>[~lib:…]*/<pfad> (Minor egal, Quelle ist der Bibliotheksordner)
  const lib = /^\/~a\/lib:([a-z0-9][a-z0-9._-]*@\d+)\.\d+(?:~lib:[a-z0-9][a-z0-9._-]*@\d+\.\d+)*\/(.+)$/.exec(p);
  if (lib || p.startsWith('/~a/lib:')) {
    const dir = lib && LIB_DIRS[lib[1]];
    if (!dir) return null;
    file = normalize(join(dir, lib[2]));
    if (!file.startsWith(dir)) return null;
  } else if (a) {
    const own = normalize(join(MODULES, a[1], a[3]));
    file = existsSync(own) ? own : normalize(join(BOS, a[3]));
  } else if (st) file = normalize(join(STUB, st[1]));
  else file = normalize(join(BOS, p));
  const okRoot = file.startsWith(BOS) || (st && file.startsWith(STUB));
  if (!okRoot || !existsSync(file) || statSync(file).isDirectory()) return null;
  return file;
}
// Antwortkörper wie der SW: .js/.mjs/.css mit ctox:shared/ oder ctox:lib/ umschreiben; binding = labBinding(URL).
export function labBody(file, bytes = readFileSync(file), binding = {}) {
  const ext = extname(file).slice(1).toLowerCase();
  if (ext !== 'js' && ext !== 'mjs' && ext !== 'css') return bytes;
  const text = bytes.toString('utf8');
  if (!text.includes('ctox:shared/') && !text.includes('ctox:lib/')) return bytes;
  return Buffer.from(SW.rewriteShellRefs(text, ext, LAB_SHELL, binding));
}
const PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Shell v4 Geometrie-Labor</title><style>html,body{margin:0;height:100%}</style></head><body><div id="ctox-shell"></div>
<script type="module">
import { createStubKernel } from '/~stub/ctox-app-api-stub.mjs';
import * as frame from '/v4/frame/index.js';
import { createWindowHost } from '/v4/shell/app-host.js';
const q = new URLSearchParams(location.search);
const app = q.get('app');
const LAB_SHELL = ${JSON.stringify(LAB_SHELL)};
const LAB_LIBS = ${JSON.stringify(LAB_LIB_BINDING)};
// Variante --shared: gemeinsame Theme-CSS der v3-Shell (shared/base.css + nur die Token-Deklarationen aus app.css),
// die der v4-Rahmen heute NICHT lädt (Entwurf §6 setzt sie voraus).
if (q.get('shared') === '1') {
  const add = (href) => new Promise((r) => { const l = document.createElement('link'); l.rel = 'stylesheet'; l.href = href; l.dataset.geoShared = ''; l.onload = l.onerror = () => r(l); document.head.append(l); });
  const appCss = await add('/app.css');
  const decl = [];
  const walk = (list) => { for (const r of list) { if (r.selectorText && ['root','html','body'].includes(r.selectorText.trim().replace(/^:/, '').split('[')[0])) { const d = [...r.style].filter((n) => n.startsWith('--')).map((n) => n + ':' + r.style.getPropertyValue(n)); if (d.length) decl.push(r.selectorText + '{' + d.join(';') + '}'); } else if (r.cssRules && !r.selectorText) walk(r.cssRules); } };
  try { walk(appCss.sheet.cssRules); } catch {}
  appCss.remove();
  const st = document.createElement('style'); st.dataset.geoShared = ''; st.textContent = decl.join(' '); document.head.append(st);
  await add('/shared/base.css');
}
const cfg = await (await fetch('/__geo/cfg/' + encodeURIComponent(q.get('cfg') || app) + '.json')).json();
window.__geo = { app, titles: [], errors: [], pushErrors: [] };
const kernel = createStubKernel({ instanceId: 'geo-' + app + '-' + Date.now(), userId: 'owner', storage: 'memory', latencyMs: 1, multiTab: false, locale: 'de-DE' });
await kernel.ready;
for (const [c, rows] of Object.entries(cfg.seed)) {
  try { await kernel.sim.push(c, rows); } catch (e) { window.__geo.pushErrors.push(c + ': ' + e.message); }
}
// Ohne Titel veröffentlichen: der Host setzt vorab nur die App-ID, jeder weitere Titel kommt von der App (G2).
// G6-Vergleichsfenster (nomin=1): ohne min_width/min_height veröffentlichen, damit ein freies Fenster in 1280 genau so
// schmal wie das Fenster im schmalen Viewport werden kann (dort ist es Mobil-Blatt = Viewport-Breite, min_width
// greift nicht). Sonst klemmt der Fenster-Manager auf min_width (meist 640) und G6 vergliche 420 mit 640.
const layout = { ...cfg.layout };
if (q.get('nomin') === '1') { delete layout.min_width; delete layout.min_height; }
// K-1d: Shell und Bibliotheken wie der Kernel veröffentlichen; artifacts.bind bindet daraus ~shell@lab~lib:… je Mount.
for (const [lib, ver] of Object.entries(LAB_LIBS)) await kernel.sim.publishAppVersion('lib:' + lib.split('@')[0], ver, { title: 'lib:' + lib, styles: [] });
// Eigentümer der Shell-Fremdlesungen nicht veröffentlichen: shell_preferences gehört ctox-platform (vom Stub selbst
// veröffentlicht, E48); eine aktive ctox-App montierte zusätzlich die Chatleiste der Shell (v4/frame/index.js syncBusinessChat).
const { _fixture_owners: _shellOwners, ...shellRoutes } = cfg.shellRoutes || {};
const { _fixture_owners: owners = [], ...routes } = cfg.routes || {};
// Ist ctox Eigentümer (aktiv), montiert der Rahmen die Chatleiste der Shell an document.body (CHAT4, v4/frame/index.js
// syncBusinessChat); G1/G5 lasten sie der gemessenen App an (wie bisher bei der ctox-App selbst).
for (const o of owners) if (o.app !== app && !(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
// Die Laborseite lädt den Rahmen direkt unter /v4/frame/ (nicht /~a/shell@…/): seine Shell-Version ist dort 'dev'
// (v4/frame/index.js startShell), kernel.preferences liest also über appApi('shell', 'dev'). E48 prüft das Lesen gegen das
// Manifest genau dieser Version; deshalb auch shell@dev mit dem echten Shell-Manifest, danach die aktive LAB_SHELL.
await kernel.sim.publishAppVersion('shell', 'dev', { title: 'Shell', styles: [], manifestExtra: shellRoutes });
await kernel.sim.publishAppVersion('shell', LAB_SHELL, { title: 'Shell', styles: [], manifestExtra: { ...shellRoutes, libs: Object.fromEntries(Object.keys(LAB_LIBS).map((k) => k.split('@'))) } });
await kernel.sim.publishAppVersion(app, '1.0.0', { entry: cfg.entry, layout, styles: cfg.styles, manifestExtra: routes });
await kernel.sim.settle();
const wrapHost = (o) => {
  const sh = o.api.shell;
  const orig = sh.setTitle;
  sh.setTitle = (t) => { window.__geo.titles.push(String(t ?? '')); return orig.call(sh, t); };
  return createWindowHost(o);
};
const shellModule = { startShell: (k, o = {}) => frame.startShell(k, { ...o, createWindowHost: wrapHost }), stopShell: () => frame.stopShell() };
await kernel.sim.startShell(shellModule);
window.__geo.kernel = kernel;
window.__geo.frame = frame;
window.__geo.bodyBefore = [...document.body.children];
window.__geo.sheetsBefore = new Set([...document.styleSheets].map((s) => s.ownerNode));
window.__geo.open = async (geometry) => {
  const h = frame.shellHandle();
  const owner = h.openWindow(app, geometry ? { geometry } : {});
  const rec = owner && h.win(owner);
  if (!rec) throw new Error('openWindow lieferte kein Fenster; kernel.apps=' + JSON.stringify(kernel.apps.get().map((x) => ({ id: x.id, active_ver: x.active_ver, activated_ver: x.activated_ver, state: x.state, error: x.error }))));
  await rec.host.ready;
  if (!geometry) rec.handle.element.querySelector('[data-window-layout-control="maximize"]')?.click();
  // Gegenprobe (--selftest): absichtlich lecke App-Regeln; G5 muss sie finden.
  if (q.get('selftest') === '1') { const st = document.createElement('style'); st.textContent = '[data-app="' + app + '"] button { color: rgb(1, 2, 3) } body { letter-spacing: 1px }'; document.head.append(st); }
  await kernel.sim.settle();
  return owner;
};
window.__geoReady = true;
</script></body></html>`;

export function serve() {
  const server = createServer((req, res) => {
    const url = new URL(req.url, 'http://geo');
    const p = decodeURIComponent(url.pathname);
    if (p === '/__geo/page.html') { res.writeHead(200, { 'content-type': 'text/html', 'cache-control': 'no-store' }); res.end(PAGE); return; }
    const cfgM = /^\/__geo\/cfg\/(.+)\.json$/.exec(p);
    if (cfgM) { res.writeHead(200, { 'content-type': 'application/json' }); res.end(JSON.stringify(configs.get(cfgM[1]) || {})); return; }
    const file = resolveLabPath(p);
    const binding = labBinding(p);
    if (!file || !binding) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream', 'cache-control': 'no-store' });
    res.end(labBody(file, undefined, binding));
  });
  return new Promise((r) => server.listen(0, '127.0.0.1', () => r(server)));
}

// ---------------------------------------------------------------------------------------------------------------
// Lauf
// ---------------------------------------------------------------------------------------------------------------
async function runOne(browser, base, id, w, { geometry = null, shot, withG5 = true, nomin = false, cfgKey = null } = {}) {
  const ctx = await browser.newContext({ viewport: { width: geometry ? BASE_W : w, height: geometry ? vh(BASE_W) : vh(w) }, deviceScaleFactor: 1, locale: 'de-DE' });
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${String(e.message || e).slice(0, 200)}`));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(`console: ${m.text().slice(0, 200)}`); });
  page.on('response', (r) => { if (r.status() >= 400) errors.push(`HTTP ${r.status()} ${r.url().replace(base, '')}`); });
  let result;
  try {
    await page.goto(`${base}/__geo/page.html?app=${encodeURIComponent(id)}${SHARED ? '&shared=1' : ''}${args.includes('--selftest') ? '&selftest=1' : ''}${nomin ? '&nomin=1' : ''}${cfgKey ? `&cfg=${encodeURIComponent(cfgKey)}` : ''}`);
    await page.waitForFunction(() => window.__geoReady === true, null, { timeout: 30000 });
    const owner = await page.evaluate((g) => window.__geo.open(g), geometry);
    await page.waitForTimeout(900); // Morph-Animation des Fenster-Managers, späte Renderschritte
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    if (shot) await page.screenshot({ path: shot });
    const pushErrors = await page.evaluate(() => window.__geo.pushErrors);
    result = await page.evaluate(measureInPage, { app: id, vocab: VOCAB, allow: configs.get(id).allow, owner, withG5 });
    result.pushErrors = pushErrors;
    const g9 = await g9Pixel(page, `.shell-window[data-window-app="${id}"]`, { label: 'Kante' });
    if (g9.skip) result.notes.push(`G9 Pixel nicht gemessen: ${g9.skip}`);
    else { result.G9.push(...g9.fails); result.info.g9 = { top: g9.row.top, left: g9.row.left, logoTop: g9.row.logoTop, edgeTop: g9.row.edgeTop }; }
    if (geometry) result.info.geometry = geometry;
    if (!geometry && w < 600) Object.assign(result, await runG11(page, owner, shot));
  } catch (e) {
    result = { G7: [], G8: [], G9: [], G11: [], G1: [`Labor-Fehler: ${String(e.message || e).split('\n')[0].slice(0, 200)}`], G2: [], G3: [], G4: [], G5: [], notes: [], info: {}, failedToRun: true };
    if (shot) { try { await page.screenshot({ path: shot }); } catch { /* egal */ } }
  }
  result.errors = [...new Set(errors)].slice(0, 12);
  await ctx.close();
  return result;
}

// G11 schmal: statisch (Spalten, Reiter, Überlauf), dann echter Tipp auf eine Listenzeile und auf „Zurück“.
const frames = (page) => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
export async function runG11(page, owner, shot) {
  const st = await page.evaluate(g11Static, { owner });
  const G11 = [...st.fails];
  const g11 = { static: st.info };
  const p0 = await page.evaluate(g11Probe, { owner, phase: 'list' });
  g11.list = p0;
  if (!p0.kit) { g11.note = 'keine Liste/Detail-Teilung (eine Pane bzw. App-eigene Navigation)'; return { G11, g11 }; }
  if (p0.mode === 'detail' && p0.listEmpty) {
    // Leere Liste: Detail zuerst (dort „Neu“ bzw. Startinhalt), „Zurück“ zeigt die leere Liste.
    const d = await page.evaluate(g11Probe, { owner, phase: 'detail' });
    g11.detail = d;
    if (shot) await page.screenshot({ path: shot.replace(/\.png$/, '-detail.png') });
    if (!d.detailShown || d.listShown) G11.push('leere Liste: Detail nicht sichtbar');
    if (!d.backShown) { G11.push('leere Liste: „Zurück“ fehlt'); return { G11, g11 }; }
    if (d.backInZone) G11.push('„Zurück“ liegt in einer reservierten Zone (G8)');
    if (d.backHit !== 'ok') G11.push(`„Zurück“ verdeckt (getroffen: ${d.backHit})`);
    if (d.zoneHits?.length) G11.push(`Detail: ${d.zoneHits.length} Bedienelemente in reservierten Zonen: ${d.zoneHits.slice(0, 3).join('; ')}`);
    await page.mouse.click(d.back.x, d.back.y);
    await page.waitForTimeout(300);
    await frames(page);
    const b = await page.evaluate(g11Probe, { owner, phase: 'back' });
    g11.back = b;
    if (b.mode !== 'list' || !b.listShown || b.detailShown) G11.push(`„Zurück“ führt nicht zur Liste (Modus ${b.mode})`);
    g11.note = 'Liste ohne Zeilen: Detail zuerst, Zurück zur leeren Liste';
    return { G11, g11 };
  }
  if (p0.mode !== 'list' || !p0.listShown || p0.detailShown) G11.push(`Grundzustand nicht Liste (Modus ${p0.mode}, Liste ${p0.listShown}, Detail ${p0.detailShown})`);
  if (!p0.row) { G11.push('keine antippbare Zeile in der Liste (Liste leer bzw. ohne Zeilen; Detail per Zeile nicht erreichbar)'); return { G11, g11 }; }
  await page.mouse.click(p0.row.x, p0.row.y);
  await page.waitForTimeout(700);
  await frames(page);
  let p1 = await page.evaluate(g11Probe, { owner, phase: 'detail' });
  if (p1.kit && p1.mode === 'list' && p0.inner) {
    g11.secondTap = 'Bedienelement der Zeile';
    await page.mouse.click(p0.inner.x, p0.inner.y);
    await page.waitForTimeout(700);
    await frames(page);
    p1 = await page.evaluate(g11Probe, { owner, phase: 'detail' });
  }
  g11.detail = p1;
  if (shot) await page.screenshot({ path: shot.replace(/\.png$/, '-detail.png') });
  if (!p1.kit) {
    // Die App hat nach dem Tipp selbst auf eine Pane umgeschaltet (eigene Liste→Detail-Navigation): das Kit tritt zurück.
    const after = await page.evaluate(g11Static, { owner });
    if (!after.fails.length && (after.info.workspaces || []).every((w) => w.visible.length === 1)) { g11.note = 'nach dem Tipp App-eigene Detailansicht (eine Pane), Kit zurückgetreten'; return { G11, g11 }; }
    G11.push(`nach dem Tipp: ${after.fails.join('; ') || 'kein Detail'}`);
    return { G11, g11 };
  }
  if (p1.mode !== 'detail' || !p1.detailShown || p1.listShown) { G11.push(`Tipp auf „${p0.row.text}“ öffnet kein Detail (Modus ${p1.mode}, Detail ${p1.detailShown}, Liste ${p1.listShown})`); return { G11, g11 }; }
  if (!p1.backShown) { G11.push('„Zurück“ im Detail nicht sichtbar'); return { G11, g11 }; }
  if (p1.backInZone) G11.push('„Zurück“ liegt in einer reservierten Zone (G8)');
  if (p1.backHit !== 'ok') G11.push(`„Zurück“ verdeckt (getroffen: ${p1.backHit})`);
  if (p1.zoneHits?.length) G11.push(`Detail: ${p1.zoneHits.length} Bedienelemente in reservierten Zonen: ${p1.zoneHits.slice(0, 3).join('; ')}`);
  await page.mouse.click(p1.back.x, p1.back.y);
  await page.waitForTimeout(300);
  await frames(page);
  const p2 = await page.evaluate(g11Probe, { owner, phase: 'back' });
  g11.back = p2;
  if (p2.mode !== 'list' || !p2.listShown || p2.detailShown) G11.push(`„Zurück“ führt nicht zur Liste (Modus ${p2.mode})`);
  if (p0.scrolled != null && Math.abs((p2.scrollTop ?? -99) - p0.scrolled) > 2) G11.push(`Listenposition nicht wiederhergestellt (${p0.scrolled} → ${p2.scrollTop})`);
  return { G11, g11 };
}

async function main() {
  mkdirSync(OUT, { recursive: true });
  const server = await serve();
  const base = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium.launch({ headless: true, ...(existsSync(EXE) ? { executablePath: EXE } : {}) });
  const results = [];
  console.log(`Stub ${STUB}\nPortiert ${PORTED.length}, übersprungen ${SKIPPED.length}: ${SKIPPED.map((s) => s.id).join(', ')}`);
  for (const { id, rest } of PORTED) {
    configs.set(id, appConfig(id));
    mkdirSync(join(OUT, id), { recursive: true });
    const css = staticCss(id);
    const perW = {};
    for (const w of WIDTHS) {
      const r = await runOne(browser, base, id, w, { shot: join(OUT, id, `${w}.png`) });
      perW[w] = r;
    }
    // G6: freies Fenster in 1280 mit der Breite des maximierten Fensters je kleinerer Breite.
    for (const w of WIDTHS.filter((x) => x < BASE_W)) {
      const ref = perW[w];
      if (ref.failedToRun || !ref.info.window) { ref.G6 = ['nicht messbar (Lauf fehlgeschlagen)']; continue; }
      const g = { x: 24, y: 16, w: ref.info.window.w, h: Math.min(ref.info.window.h, vh(BASE_W) - 90) };
      // Vergleichsfenster ohne min_width (nomin), sonst wäre es bei 420 so breit wie min_width (meist 640).
      const r2 = await runOne(browser, base, id, w, { geometry: g, shot: join(OUT, id, `g6-${w}-in-1280.png`), withG5: false, nomin: true });
      const refW = ref.info.window.w, cmpW = r2.info?.window?.w;
      const cmp = sameLayout(ref.signature, r2.signature);
      // Nur gleich breite Fenster sind vergleichbar; weicht die Breite ab, ist G6 nicht gemessen (rot, mit Grund).
      if (!(Math.abs((cmpW ?? -99) - refW) <= 2)) ref.G6 = [`nicht vergleichbar: Vergleichsfenster in 1280 ist ${cmpW ?? '–'} px breit statt ${refW} px`];
      else ref.G6 = cmp.same ? [] : [cmp.why];
      ref.g6 = { jaccard: cmp.j, windowColumns: r2.signature?.columns, viewportColumns: ref.signature?.columns, viewportWindowW: refW, compareWindowW: cmpW ?? null };
    }
    for (const w of WIDTHS) {
      const r = perW[w];
      r.G6 = r.G6 || [];
      if (css.media.length) r.G6.push(`App-CSS nutzt @media mit Breite (Viewport statt Fenster): ${css.media.slice(0, 4).join(', ')}`);
      const crit = ['G1', 'G2', 'G3', 'G4', 'G5', 'G6', 'G7', 'G8', 'G9', 'G11'];
      for (const k of crit) r[k] = r[k] || [];
      const fails = crit.filter((k) => r[k].length);
      results.push({ app: id, width: w, ok: fails.length === 0, fails, scanRest: rest, containerBreakpoints: css.containers, ...Object.fromEntries(crit.map((k) => [k, r[k]])), notes: r.notes, info: r.info, g6: r.g6 || null, g11: r.g11 || null, errors: r.errors, pushErrors: r.pushErrors || [], shot: relative(OUT, join(OUT, id, `${w}.png`)) });
      console.log(`${fails.length ? 'FAIL' : 'OK  '} ${String(w).padStart(4)} ${id.padEnd(16)} ${fails.join(',')}${r.errors?.length ? `  (${r.errors.length} Seitenfehler)` : ''}`);
    }
  }
  // Zonen-Fälle: Fehlerzustand gezielt, Zone erwartet (nur Apps dieses Laufs, Breite 1280).
  const zoneCases = [];
  for (const c of ZONE_CASES.filter((x) => PORTED.some((p) => p.id === x.app))) {
    const key = `${c.app}~${c.name}`;
    const cfg = configs.get(c.app) || appConfig(c.app);
    configs.set(key, { ...cfg, seed: c.patch(structuredClone(cfg.seed)) });
    const shot = join(OUT, c.app, `zone-${c.name}-${BASE_W}.png`);
    const r = await runOne(browser, base, c.app, BASE_W, { shot, withG5: false, cfgKey: key });
    const j = judgeZoneCase(c, r);
    zoneCases.push({ app: c.app, case: c.name, width: BASE_W, expect: c.expect, ok: j.ok, fails: j.fails, zoneEntries: r.info?.zoneEntries || [], G3: r.G3, errors: r.errors, shot: relative(OUT, shot) });
    console.log(`${j.ok ? 'OK  ' : 'FAIL'} zone ${c.app}/${c.name} (${c.expect})${j.ok ? '' : `: ${j.fails.join('; ')}`}`);
  }
  await browser.close();
  server.close();
  // Teillauf (--app): Ergebnisse anderer Apps aus einem vorhandenen results.json übernehmen.
  if (ONLY && existsSync(join(OUT, 'results.json'))) {
    try {
      const prev = JSON.parse(readFileSync(join(OUT, 'results.json'), 'utf8'));
      const keep = (prev.results || []).filter((x) => !ONLY.includes(x.app));
      results.unshift(...keep);
      results.sort((a, b) => a.app.localeCompare(b.app) || b.width - a.width);
      for (const id of prev.ported || []) if (!PORTED.some((p) => p.id === id)) PORTED.push({ id });
      PORTED.sort((a, b) => a.id.localeCompare(b.id));
    } catch { /* kein gültiger Vorlauf */ }
  }
  if (ONLY && existsSync(join(OUT, 'results.json'))) {
    try { const prev = JSON.parse(readFileSync(join(OUT, 'results.json'), 'utf8')); zoneCases.unshift(...(prev.zoneCases || []).filter((x) => !ONLY.includes(x.app))); } catch { /* egal */ }
  }
  const summary = { at: new Date().toISOString(), variant: SHARED ? 'shared-css' : 'frame', stub: STUB, widths: WIDTHS, ported: PORTED.map((p) => p.id), skipped: SKIPPED.map((s) => ({ id: s.id, reason: s.reason })), results, zoneCases };
  writeFileSync(join(OUT, 'results.json'), JSON.stringify(summary, null, 1));
  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
  const gallery = `<!doctype html><meta charset="utf-8"><title>Shell v4 Geometrie</title>
<body style="font-family:system-ui;background:#f4f5f7;color:#1d232b;padding:16px;max-width:1500px">
<h1>Shell v4 Geometrie · ${esc(summary.at.slice(0, 16))} · ${results.filter((r) => r.ok).length}/${results.length} OK</h1>
${PORTED.map(({ id }) => `<section><h2>${esc(id)}</h2><div style="display:flex;gap:12px;align-items:flex-start;flex-wrap:wrap">${results.filter((r) => r.app === id).map((r) => `<figure style="margin:0;max-width:${r.width === 420 ? 260 : 560}px"><img src="${esc(r.shot)}" style="width:100%;border:1px solid #ccd"><figcaption style="font-size:12px;color:${r.ok ? '#276' : '#b33'}">${r.width}px ${r.ok ? 'OK' : esc(r.fails.join(', '))}</figcaption></figure>`).join('')}</div></section>`).join('\n')}
</body>`;
  writeFileSync(join(OUT, 'index.html'), gallery);
  console.log(`\n${results.filter((r) => r.ok).length}/${results.length} OK · Zonen-Fälle ${zoneCases.filter((z) => z.ok).length}/${zoneCases.length} · ${join(OUT, 'results.json')} · ${join(OUT, 'index.html')}`);
  // Als CI-Gate (crew-ui v4-Linie) muss ein roter Befund den Lauf rot machen; bis 17.10. endete er immer mit 0.
  process.exitCode = results.every((r) => r.ok) && zoneCases.every((z) => z.ok) ? 0 : 1;
}

if (IS_MAIN) main().catch((e) => { console.error(e); process.exit(1); });
