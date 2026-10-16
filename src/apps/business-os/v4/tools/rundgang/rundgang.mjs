#!/usr/bin/env node
// Shell v4 – Bedienrundgang (Strang RUND1, wiederverwendbar für RUND2).
//
// Startet je App und Breite den echten v4-Rahmen (v4/frame + v4/shell/app-host) gegen den Kern-Stub mit den Fixtures
// des Geometrie-Labors (appConfig/fixtures.mjs), öffnet die App über den LAUNCHER (echter Mausklick auf die Kachel),
// maximiert per Klick auf den Fensterknopf und bedient danach jedes erreichbare Bedienelement im App-Inhalt mit
// page.mouse.click (Reiter, Navigation, Knöpfe, Schubladen, Menüs; neu erscheinende Elemente kommen in die Warteschlange).
// Vor Speichern/Anlegen-Knöpfen werden leere Textfelder per Tastatur gefüllt; Suchfelder werden mit einem Fixture-Wort
// bedient. Destruktive Knöpfe (Löschen/Entfernen/Abmelden …) werden NICHT geklickt (Fixture bleibt vergleichbar).
//
//   node v4/tools/rundgang/rundgang.mjs --app <id>[,<id>…] [--widths 1280,420] [--out <dir>] [--max 28] [--tag RUND1]
//
// Je Schritt protokolliert: Ziel (Rolle, Beschriftung), Wirkung (Text-/DOM-Änderung im App-Inhalt), Schreibvorgänge
// (api.put/patch/remove), gesendete Befehle (api.command + Endstatus), Shell-Dialoge/Menüs/Navigation/openFile,
// Zustandszone (Einträge der App + sichtbar ja/nein), Seitenfehler, Geometrie (horizontales Scrollen des Inhalts,
// sichtbare App-Elemente außerhalb des Fensters, neue Kinder an document.body). Screenshot je Schritt (JPEG) und
// ein Kontaktbogen je App×Breite (<out>/<app>/sheet-<w>.png) zum Ansehen.
//
// Ausgaben: <out>/<app>/<w>-<nn>.jpg, <out>/<app>/sheet-<w>.png, <out>/<app>/steps-<w>.json, <out>/summary.json
// (zusammengeführt bei Teilläufen). Befunde (auffällige Schritte) stehen je Lauf unter `flags`.
import { createServer } from 'node:http';
import { existsSync, mkdirSync, writeFileSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { join, dirname, extname, normalize } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { appConfig, configs, resolveLabPath, labBinding, labBody } from '../geometry/geometry-v4.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const opt = (name, dflt) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : dflt; };
const APPS = String(opt('--app', '')).split(',').filter(Boolean);
const WIDTHS = String(opt('--widths', '1280,420')).split(',').map(Number).filter(Boolean);
const TAG = opt('--tag', 'RUND');
const OUT = normalize(opt('--out', join(HERE, 'shots', TAG)));
const MAX = Number(opt('--max', '34'));
const HEIGHT = (w) => (w <= 480 ? 860 : 800);
if (!APPS.length && process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) { console.error('Aufruf: node rundgang.mjs --app <id>[,<id>…] [--widths 1280,420] [--out dir] [--max 28]'); process.exit(64); }

const require = createRequire(join(homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const EXE = join(homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.jpg': 'image/jpeg', '.woff2': 'font/woff2', '.woff': 'font/woff', '.ttf': 'font/ttf', '.wasm': 'application/wasm', '.map': 'application/json', '.txt': 'text/plain' };

// Destruktiv / verlässt die Sitzung: nicht klicken.
const SKIP_RE = /(lösch|loesch|delete|entfern|remove|abmelden|logout|sign out|zurücksetzen|widerruf|revoke|deinstall|uninstall|papierkorb leeren|verwerf|discard|purge|vernicht)/i;
const SAVE_RE = /(speichern|save|anlegen|erstellen|create|hinzufügen|add|übernehmen|apply|senden|send|einreichen|submit|bestätigen|confirm|importieren|import|starten|start|ausführen|run|generieren|generate)/i;
const SEARCH_WORD = 'Müller';

// ---------------------------------------------------------------------------------------------------------------
// Laborseite: wie geometry-v4.mjs, aber OHNE automatisches Öffnen (Launcher-Klick) und mit Protokoll der App-API.
// ---------------------------------------------------------------------------------------------------------------
const PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Shell v4 Rundgang</title><style>html,body{margin:0;height:100%}</style></head><body><div id="ctox-shell"></div>
<script type="module">
import { createStubKernel } from '/~stub/ctox-app-api-stub.mjs';
import * as frame from '/v4/frame/index.js';
import { createWindowHost } from '/v4/shell/app-host.js';
const q = new URLSearchParams(location.search);
const app = q.get('app');
const cfg = await (await fetch('/__rg/cfg/' + encodeURIComponent(app) + '.json')).json();
const L = window.__rg = { app, log: [], titles: [], pushErrors: [] };
const kernel = createStubKernel({ instanceId: 'rg-' + app + '-' + Date.now(), userId: 'owner', storage: 'memory', latencyMs: 1, multiTab: false, locale: 'de-DE' });
await kernel.ready;
for (const [c, rows] of Object.entries(cfg.seed)) { try { await kernel.sim.push(c, rows); } catch (e) { L.pushErrors.push(c + ': ' + e.message); } }
// K-1d: artifacts.bind je Mount braucht eine aktive Shell im Kernel (wie rundgang2.mjs).
try { await kernel.sim.publishAppVersion('shell', 'lab', { entry: 'v4/frame/index.js' }); } catch (e) { L.pushErrors.push('shell: ' + e.message); }
await kernel.sim.publishAppVersion(app, '1.0.0', { title: cfg.title || undefined, entry: cfg.entry, layout: cfg.layout, styles: cfg.styles });
// Übrige Apps nur veröffentlichen (Launcher realistisch, shell.navigate-Ziele vorhanden); geöffnet wird nur die geprüfte.
for (const o of (await (await fetch('/__rg/others.json')).json())) if (o.id !== app) { try { await kernel.sim.publishAppVersion(o.id, '1.0.0', o); } catch {} }
await kernel.sim.settle();
const short = (v) => { try { const s = JSON.stringify(v); return s && s.length > 240 ? s.slice(0, 240) + '…' : s; } catch { return String(v); } };
const wrapData = (data) => {
  const api = { ...data };
  for (const m of ['put', 'patch', 'remove']) {
    const f = data[m]; if (typeof f !== 'function') continue;
    api[m] = (c, ...rest) => {
      const h = f.call(data, c, ...rest);
      const e = { kind: 'write', op: m, collection: c, id: rest[0]?.id ?? rest[0], data: short(rest[m === 'patch' ? 1 : 0]) };
      L.log.push(e);
      try { h?.result?.then((r) => { e.result = r?.status + (r?.reason ? ':' + r.reason : ''); }); } catch {}
      return h;
    };
  }
  if (typeof data.command === 'function') api.command = (type, payload, o2) => {
    const h = data.command(type, payload, o2);
    const e = { kind: 'command', type, payload: short(payload), status: 'queued' };
    L.log.push(e);
    try { h?.status?.subscribe?.((s) => { e.status = s?.status; if (s?.error) e.error = String(s.error); }); } catch {}
    return h;
  };
  if (data.files?.put) { const fp = data.files.put; api.files = { ...data.files, put: (b, meta) => { L.log.push({ kind: 'files.put', meta: short(meta) }); return fp.call(data.files, b, meta); } }; }
  if (data.shell) {
    api.shell = { ...data.shell };
    for (const m of ['dialog', 'menu', 'openFile']) {
      const f = data.shell[m]; if (typeof f !== 'function') continue;
      api.shell[m] = (...a) => { L.log.push({ kind: 'shell.' + m, spec: short(a[0]) }); return f.apply(data.shell, a); };
    }
  }
  return api;
};
const wrapHost = (o) => {
  const sh = o.api.shell;
  if (!sh.__rg) {
    sh.__rg = true;
    const t0 = sh.setTitle; sh.setTitle = (t) => { L.titles.push(String(t ?? '')); return t0.call(sh, t); };
    const n0 = sh.navigate; if (typeof n0 === 'function') sh.navigate = (t, p) => { L.log.push({ kind: 'shell.navigate', spec: short([t, p]) }); return n0.call(sh, t, p); };
  }
  const k = o.api.kernel;
  if (k && !k.__rg) { k.__rg = true; const a0 = k.appApi.bind(k); k.appApi = (id, ver) => wrapData(a0(id, ver)); }
  return createWindowHost(o);
};
const shellModule = { startShell: (k, o = {}) => frame.startShell(k, { ...o, createWindowHost: wrapHost }), stopShell: () => frame.stopShell() };
await kernel.sim.startShell(shellModule);
L.kernel = kernel; L.frame = frame;
L.bodyBefore = new Set(document.body.children);
window.__rgReady = true;
</script></body></html>`;

// Alle Apps mit v4-Manifest (nur Veröffentlichungsdaten, keine Fixtures).
const MANI = join(HERE, '..', '..', 'manifests');
const OTHERS = readdirSync(MANI).filter((f) => f.endsWith('.manifest.json')).map((f) => {
  try {
    const m = JSON.parse(readFileSync(join(MANI, f), 'utf8'));
    const id = f.replace(/\.manifest\.json$/, '');
    const layout = {};
    for (const k of ['icon_svg', 'icon_asset', 'category', 'default_width', 'default_height', 'min_width', 'min_height']) if (m.layout?.[k] != null) layout[k] = m.layout[k];
    return { id, title: m.title || undefined, entry: m.entry || 'index.js', layout, styles: Array.isArray(m.styles) ? m.styles : [] };
  } catch { return null; }
}).filter((o) => o && !/^(lib-|shell)/.test(o.id));

function serve() {
  const server = createServer((req, res) => {
    const url = new URL(req.url, 'http://rg');
    const p = decodeURIComponent(url.pathname);
    if (p === '/__rg/others.json') { res.writeHead(200, { 'content-type': 'application/json' }); res.end(JSON.stringify(OTHERS)); return; }
    if (p === '/__rg/page.html') { res.writeHead(200, { 'content-type': 'text/html', 'cache-control': 'no-store' }); res.end(PAGE); return; }
    const cfgM = /^\/__rg\/cfg\/(.+)\.json$/.exec(p);
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
// In der Seite: Bedienelemente des App-Inhalts und Zustand
// ---------------------------------------------------------------------------------------------------------------
function pageCandidates() {
  const win = document.querySelector(`[data-window-app="${window.__rg.app}"]`);
  const root = win?.querySelector('.v4-app-root');
  if (!root) return [];
  for (const old of document.querySelectorAll('[data-rg-idx]')) delete old.dataset.rgIdx;
  const W = win.getBoundingClientRect();
  const vis = (el) => el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true }) && (() => { const r = el.getBoundingClientRect(); return r.width >= 4 && r.height >= 4; })();
  const sel = 'button, [role=tab], [role=button], [role=menuitem], [role=option], [role=treeitem], a[href], summary, select, input[type=checkbox], input[type=radio], [data-action], [onclick], li[tabindex], tr[tabindex], [role=row][tabindex], [role=link]';
  const out = [];
  const seen = new Map();
  const scopes = [root, ...[...win.querySelectorAll('[data-window-actions] > *')].filter((x) => !x.matches('[data-window-layout-control], .shell-window-controls') && !x.closest('[data-window-layout-control]'))];
  const pool = scopes.flatMap((s) => [...(s.matches(sel) && s !== root ? [s] : []), ...s.querySelectorAll(sel)]);
  // Klickbare Zeilen ohne Rolle (Listeneinträge mit Klick-Handler): erkennbar am Zeiger-Cursor, ohne interaktiven
  // Vorfahren und ohne eigenen Knopf darin; höchstens 12 je Aufruf.
  let extra = 0;
  for (const el of root.querySelectorAll('div, li, article, tr, section')) {
    if (extra >= 12) break;
    if (getComputedStyle(el).cursor !== 'pointer') continue;
    if (el.parentElement && getComputedStyle(el.parentElement).cursor === 'pointer') continue;
    if (el.closest(sel) || el.querySelector(sel)) continue;
    pool.push(el); extra++;
  }
  for (const el of pool) {
    if (el.closest('[data-window-layout-control]')) continue;
    if (el.closest('[disabled],[aria-disabled=true],[inert]') || el.disabled) continue;
    if (!vis(el)) continue;
    const r = el.getBoundingClientRect();
    const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
    // Außerhalb des sichtbaren Fensterteils (z. B. unter dem Falz eines Formulars): erreichbar per Scrollen.
    const offscreen = cx < W.left || cx > W.right || cy < W.top || cy > W.bottom;
    const hit = offscreen ? null : document.elementFromPoint(cx, cy);
    const inner = el.matches('button,[role=button],[role=tab],a') ? el : (el.querySelector('button,[role=button],a') ? null : el);
    if (!inner) continue; // Container mit eigenem Knopf: der Knopf selbst kommt einzeln
    const role = el.getAttribute('role') || el.tagName.toLowerCase() + (el.type && el.tagName === 'INPUT' ? ':' + el.type : '');
    const label = (el.getAttribute('aria-label') || el.getAttribute('title') || el.querySelector(':scope > title')?.textContent || el.innerText || el.value || el.getAttribute('data-action') || '').replace(/\s+/g, ' ').trim().slice(0, 60);
    const base = `${role}|${label}`;
    const n = (seen.get(base) || 0) + 1; seen.set(base, n);
    el.dataset.rgIdx = String(out.length);
    out.push({ idx: out.length, key: `${base}|${n}`, role, label, x: Math.round(cx), y: Math.round(cy), offscreen, covered: !offscreen && !(hit && (hit === el || el.contains(hit))), href: el.tagName === 'A' ? el.getAttribute('href') : null, target: el.getAttribute('target') || '' });
  }
  return out;
}

function pageState() {
  const L = window.__rg;
  const win = document.querySelector(`[data-window-app="${L.app}"]`);
  const root = win?.querySelector('.v4-app-root');
  const st = { hasWindow: !!win, hasRoot: !!root };
  if (!root) return st;
  const W = win.getBoundingClientRect();
  st.window = { x: Math.round(W.left), y: Math.round(W.top), w: Math.round(W.width), h: Math.round(W.height), mobile: win.classList.contains('is-mobile-sheet') };
  st.text = (root.innerText || '').replace(/\s+/g, ' ').trim();
  st.textHash = st.text.length + ':' + [...st.text].reduce((h, c) => (h * 31 + c.charCodeAt(0)) | 0, 7);
  st.nodes = root.querySelectorAll('*').length;
  st.hscroll = [];
  if (root.scrollWidth > root.clientWidth + 1) st.hscroll.push(`root ${root.scrollWidth}>${root.clientWidth}`);
  const vis = (el) => el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true });
  for (const el of root.querySelectorAll('*')) {
    if (el.scrollWidth > el.clientWidth + 1 && /(auto|scroll)/.test(getComputedStyle(el).overflowX) && !el.closest('[data-scroll-x]') && vis(el) && el.clientWidth > 40) {
      st.hscroll.push(`${el.tagName.toLowerCase()}${el.getAttribute('class') ? '.' + el.getAttribute('class').trim().split(/\s+/)[0] : ''} ${el.scrollWidth}>${el.clientWidth}`);
      if (st.hscroll.length > 4) break;
    }
  }
  // sichtbare App-Elemente, die über den Fensterrand hinausragen (ohne Clipping durch overflow)
  st.outside = [];
  const R = root.getBoundingClientRect();
  for (const el of root.querySelectorAll('*')) {
    if (st.outside.length > 4) break;
    const r = el.getBoundingClientRect();
    if (r.width * r.height <= 4 || !vis(el)) continue;
    if (r.right > W.right + 1 || r.left < W.left - 1 || r.bottom > W.bottom + 1) {
      // nur wenn tatsächlich sichtbar (Punktprobe auf den überstehenden Teil)
      const px = r.right > W.right + 1 ? Math.min(innerWidth - 1, W.right + 3) : (r.left < W.left - 1 ? Math.max(0, W.left - 3) : (r.left + r.right) / 2);
      const py = r.bottom > W.bottom + 1 ? Math.min(innerHeight - 1, W.bottom + 3) : (r.top + r.bottom) / 2;
      // Probepunkt muss wirklich außerhalb des Fensters liegen (am Viewport-Rand geklemmt fällt er sonst ins Fenster).
      if (px >= W.left && px <= W.right && py >= W.top && py <= W.bottom) continue;
      const hit = document.elementFromPoint(px, py);
      if (hit && (hit === el || el.contains(hit))) st.outside.push(`${el.tagName.toLowerCase()}.${String(el.getAttribute('class') || '').split(' ')[0]} ${Math.round(r.left)}..${Math.round(r.right)}x${Math.round(r.bottom)} (Fenster ${Math.round(W.left)}..${Math.round(W.right)}x${Math.round(W.bottom)})`);
    }
  }
  st.bodyNew = [...document.body.children].filter((c) => !L.bodyBefore.has(c) && c.tagName !== 'SCRIPT' && vis(c)).map((c) => `${c.tagName.toLowerCase()}.${String(c.getAttribute('class') || '').split(' ')[0]}`);
  const zone = win.querySelector('.v4-status-zone');
  st.zoneVisible = !!(zone && !zone.hidden && zone.getBoundingClientRect().height > 0);
  st.zoneText = st.zoneVisible ? (zone.innerText || '').replace(/\s+/g, ' ').trim().slice(0, 160) : '';
  try { st.zone = (L.kernel.status.forApp ? L.kernel.status.forApp(L.app).get() : L.kernel.status.get().filter((e) => e.app_id === L.app)).map((e) => `${e.source}:${e.code}${e.text ? ' ' + String(e.text).slice(0, 80) : ''}`); } catch { st.zone = []; }
  st.logLen = L.log.length;
  st.title = L.titles[L.titles.length - 1] || '';
  st.dialogs = L.kernel.sim.dialogs.length;
  st.navigations = L.kernel.sim.navigations.length;
  st.openedFiles = L.kernel.sim.openedFiles.length;
  st.focusInApp = root.contains(document.activeElement);
  return st;
}

// ---------------------------------------------------------------------------------------------------------------
// Lauf je App × Breite
// ---------------------------------------------------------------------------------------------------------------
async function settle(page, ms = 350) {
  await page.waitForTimeout(ms);
  await page.evaluate(async () => { try { await window.__rg.kernel.sim.settle(); } catch {} await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))); });
}

async function fillEmptyFields(page) {
  const fields = await page.evaluate(() => {
    const root = document.querySelector('.v4-app-root');
    if (!root) return [];
    const out = [];
    for (const el of root.querySelectorAll('input:not([type]), input[type=text], input[type=email], input[type=url], input[type=number], input[type=tel], textarea')) {
      if (el.disabled || el.readOnly || el.value) continue;
      if (!el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })) continue;
      const r = el.getBoundingClientRect();
      if (r.width < 8 || r.height < 8) continue;
      out.push({ x: r.left + Math.min(20, r.width / 2), y: r.top + r.height / 2, type: el.type || 'text', name: el.name || el.placeholder || el.getAttribute('aria-label') || '' });
      if (out.length >= 6) break;
    }
    return out;
  });
  for (const f of fields) {
    await page.mouse.click(f.x, f.y);
    const v = f.type === 'number' ? '42' : f.type === 'email' ? 'rundgang@example.de' : f.type === 'url' ? 'https://example.de' : 'Rundgang Test';
    await page.keyboard.type(v, { delay: 5 });
  }
  return fields.map((f) => f.name || f.type);
}

async function searchFields(page) {
  return page.evaluate(() => {
    const root = document.querySelector('.v4-app-root');
    if (!root) return [];
    return [...root.querySelectorAll('input[type=search], input[placeholder], input[aria-label]')].filter((el) => {
      const s = `${el.type} ${el.placeholder} ${el.getAttribute('aria-label') || ''}`;
      if (!/search|such|filter|find/i.test(s)) return false;
      if (!el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })) return false;
      const r = el.getBoundingClientRect();
      return r.width >= 8 && r.height >= 8;
    }).slice(0, 2).map((el) => { const r = el.getBoundingClientRect(); return { x: r.left + Math.min(30, r.width / 2), y: r.top + r.height / 2, label: el.placeholder || el.getAttribute('aria-label') || el.type }; });
  });
}

async function runOne(browser, base, id, w) {
  const dir = join(OUT, id);
  mkdirSync(dir, { recursive: true });
  for (const f of readdirSync(dir)) if (f.startsWith(`${w}-`)) rmSync(join(dir, f));
  const ctx = await browser.newContext({ viewport: { width: w, height: HEIGHT(w) }, deviceScaleFactor: 1, locale: 'de-DE' });
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${String(e.message || e).slice(0, 200)}`));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(`console: ${m.text().slice(0, 1500)}`); });
  page.on('response', (r) => { if (r.status() >= 400) errors.push(`HTTP ${r.status()} ${r.url().replace(base, '')}`); });
  const downloads = [];
  page.on('download', (d) => downloads.push(d.suggestedFilename()));
  page.on('filechooser', (fc) => downloads.push(`Dateiauswahl geöffnet (${fc.isMultiple() ? 'mehrere' : 'eine'} Datei)`));
  page.on('popup', (p) => { errors.push(`popup: ${p.url()}`); p.close().catch(() => {}); });
  await page.route((u) => !u.href.startsWith(base) && !u.href.startsWith('blob:') && !u.href.startsWith('data:'), (r) => { errors.push(`extern blockiert: ${r.request().url().slice(0, 120)}`); r.abort(); });
  const steps = [];
  const flags = [];
  let n = 0;
  const shot = async (label) => {
    const file = `${w}-${String(n).padStart(2, '0')}.jpg`;
    await page.screenshot({ path: join(dir, file), type: 'jpeg', quality: 62 });
    n++;
    return file;
  };
  const record = async (step, before) => {
    const after = await page.evaluate(pageState);
    const log = await page.evaluate((from) => window.__rg.log.slice(from).map((e) => ({ ...e })), before?.logLen ?? 0);
    const errs = errors.splice(0);
    for (const f of downloads.splice(0)) log.push({ kind: /^Dateiauswahl/.test(f) ? 'filechooser' : 'download', spec: f });
    const s = { ...step, shot: await shot(step.label), effect: before ? (after.textHash !== before.textHash ? 'Inhalt geändert' : (after.nodes !== before.nodes ? 'DOM geändert' : 'keine sichtbare Wirkung')) : '', log, errors: errs, title: after.title, zone: after.zone, zoneVisible: after.zoneVisible, zoneText: after.zoneText, hscroll: after.hscroll, outside: after.outside, bodyNew: after.bodyNew, window: after.window, dialogs: after.dialogs - (before?.dialogs ?? 0), navigations: after.navigations - (before?.navigations ?? 0), openedFiles: after.openedFiles - (before?.openedFiles ?? 0) };
    if (!after.hasRoot && step.role !== 'launcher') s.problem = 'App-Inhalt weg (kein .v4-app-root)';
    const probs = [];
    if (s.errors.some((e) => /^pageerror/.test(e) || (/^console/.test(e) && !/Failed to load resource/.test(e)) || (/^HTTP/.test(e) && e.includes(`/~a/${id}@`)))) probs.push('Seitenfehler');
    if (s.hscroll?.length) probs.push('horizontal scrollbar');
    if (s.outside?.length) probs.push('App-Element außerhalb des Fensters');
    if (s.bodyNew?.length) probs.push('Element an document.body');
    if (s.zoneVisible) probs.push('Zone sichtbar');
    if (s.problem) probs.push(s.problem);
    if (log.some((e) => e.kind === 'command' && e.status === 'failed')) probs.push('Befehl failed');
    if (log.some((e) => e.kind === 'write' && e.result && !/^accepted/.test(e.result))) probs.push('Write nicht accepted');
    s.probs = probs;
    steps.push(s);
    if (probs.length) flags.push({ step: steps.length - 1, label: s.label, probs, shot: s.shot });
    return after;
  };

  try {
    await page.goto(`${base}/__rg/page.html?app=${encodeURIComponent(id)}`);
    await page.waitForFunction(() => window.__rgReady === true, null, { timeout: 30000 });
    await settle(page, 300);
    // 1. Launcher: Kachel suchen, sichtbar machen, echter Klick
    const tile = page.locator(`[data-v4-app="${id}"] [data-v4-open]`).first();
    await tile.scrollIntoViewIfNeeded({ timeout: 5000 });
    const tb = await tile.boundingBox();
    await record({ label: 'Launcher', role: 'launcher' }, null);
    if (!tb) throw new Error('Launcher-Kachel nicht sichtbar');
    await page.mouse.click(tb.x + tb.width / 2, tb.y + tb.height / 2);
    await page.waitForSelector('.v4-app-root', { timeout: 12000 });
    await settle(page, 900);
    // 2. Maximieren per Klick (falls Knopf sichtbar; mobil ist das Fenster ein Blatt)
    const clickVisible = async (sel) => {
      const loc = page.locator(`[data-window-app="${id}"] ${sel}`).first();
      if (!(await loc.isVisible().catch(() => false))) return false;
      const b = await loc.boundingBox(); if (!b) return false;
      await page.mouse.click(b.x + b.width / 2, b.y + b.height / 2); await settle(page, 300); return true;
    };
    if (!(await clickVisible('[data-window-layout-control="maximize"]'))) {
      if (await clickVisible('[data-window-layout-control="toggle"]')) await clickVisible('[data-window-layout-control="maximize"]');
      else await clickVisible('[data-window-control="maximize"]');
    }
    await settle(page, 600);
    let state = await record({ label: 'geöffnet', role: 'open' }, null);
    // 3. Suche
    const word = await page.evaluate(async () => {
      const text = (document.querySelector('.v4-app-root')?.innerText || '');
      const cfg = await (await fetch('/__rg/cfg/' + encodeURIComponent(window.__rg.app) + '.json')).json();
      for (const rows of Object.values(cfg.seed || {})) for (const r of rows) for (const v of Object.values(r)) {
        if (typeof v !== 'string' || v.length < 4 || v.length > 60 || /^[a-z0-9_-]+$/.test(v)) continue;
        const w = v.split(/[\s,]+/).find((x) => x.length >= 4 && /\p{Lu}/u.test(x[0]));
        if (w && text.includes(w)) return w;
      }
      // Rückfall: ein großgeschriebenes Wort aus einer sichtbaren Inhaltszeile (Listeneintrag), nicht aus Platzhaltern.
      const lines = text.split('\n').map((l) => l.trim()).filter((l) => l.length > 3);
      for (const l of lines.slice(3)) { const w = l.split(/[\s,()]+/).find((x) => x.length >= 4 && /^\p{Lu}\p{Ll}+$/u.test(x)); if (w) return w; }
      return null;
    }) || SEARCH_WORD;
    for (const f of await searchFields(page)) {
      const before = state;
      await page.mouse.click(f.x, f.y);
      await page.keyboard.type(word, { delay: 10 });
      await settle(page, 400);
      state = await record({ label: `Suche „${word}“ in ${f.label}`, role: 'search' }, before);
      const b2 = state;
      await page.mouse.click(f.x, f.y);
      await page.keyboard.press(process.platform === 'darwin' ? 'Meta+A' : 'Control+A');
      await page.keyboard.press('Backspace');
      await settle(page, 300);
      state = await record({ label: 'Suche geleert', role: 'search' }, b2);
    }
    // 4. Bedienelemente abarbeiten (Breitensuche; neu erscheinende kommen hinten dran)
    const done = new Set();
    const deferred = new Set();
    let escTried = false;
    const queue = (await page.evaluate(pageCandidates)).map((c) => c.key);
    let clicks = 0;
    while (queue.length && clicks < MAX) {
      const key = queue.shift();
      if (done.has(key)) continue;
      done.add(key);
      const cands = await page.evaluate(pageCandidates);
      const c = cands.find((x) => x.key === key);
      if (!c) continue;
      // Verdeckt (Dialog/Schublade liegt darüber): später erneut versuchen, höchstens einmal.
      if (c.covered && !escTried) {
        // Überlagerung offen (Dialog/Schublade/Menü): wie ein Nutzer mit Escape schließen, dann neu prüfen.
        escTried = true;
        const b0 = state;
        await page.keyboard.press('Escape');
        await settle(page, 300);
        state = await record({ label: 'Escape (Überlagerung schließen)', role: 'key' }, b0);
        const again2 = (await page.evaluate(pageCandidates)).find((x) => x.key === key);
        if (again2 && !again2.covered) { done.delete(key); queue.unshift(key); continue; }
      }
      if (c.covered) { if (!deferred.has(key)) { deferred.add(key); done.delete(key); queue.push(key); } else steps.push({ label: c.label, role: c.role, skipped: 'verdeckt – nicht erreichbar' }); continue; }
      if (SKIP_RE.test(c.label)) { steps.push({ label: c.label, role: c.role, skipped: 'destruktiv – nicht geklickt' }); continue; }
      if (c.href && (/^https?:/i.test(c.href) || c.target === '_blank')) { steps.push({ label: c.label, role: c.role, skipped: `externer Link ${c.href}` }); continue; }
      let filled = [];
      if (SAVE_RE.test(c.label)) filled = await fillEmptyFields(page);
      let again = filled.length ? (await page.evaluate(pageCandidates)).find((x) => x.key === key) || c : c;
      if (again.offscreen) {
        // wie ein Nutzer: Element in den sichtbaren Bereich scrollen, dann neu vermessen
        try { await page.locator(`[data-rg-idx="${again.idx}"]`).first().scrollIntoViewIfNeeded({ timeout: 2000 }); } catch { /* egal */ }
        await settle(page, 150);
        const re = (await page.evaluate(pageCandidates)).find((x) => x.key === key);
        if (!re || re.offscreen || re.covered) { steps.push({ label: c.label, role: c.role, skipped: re?.covered ? 'nach Scrollen verdeckt' : 'auch nach Scrollen nicht im Fenster' }); continue; }
        again = re;
      }
      const before = state;
      const urlBefore = page.url();
      if (again.role === 'select') {
        const loc = page.locator(`[data-rg-idx="${again.idx}"]`).first();
        const next = await loc.evaluate((el) => (el.options ? (el.selectedIndex + 1) % Math.max(1, el.options.length) : -1));
        await page.mouse.click(again.x, again.y);
        await page.keyboard.press('Escape');
        if (next >= 0) { try { await loc.selectOption({ index: next }, { timeout: 3000 }); c.label = `${c.label} → Option ${next}`; } catch (e) { c.label = `${c.label} → Auswahl nicht möglich (${String(e.message).split('\n')[0].slice(0, 80)})`; } }
      } else await page.mouse.click(again.x, again.y);
      clicks++;
      escTried = false;
      await settle(page, 450);
      if (page.url() !== urlBefore) { steps.push({ label: c.label, role: c.role, problem: `Seite navigiert nach ${page.url()}` }); flags.push({ step: steps.length - 1, label: c.label, probs: ['Seitennavigation'] }); break; }
      state = await record({ label: c.label || '(ohne Beschriftung)', role: c.role, covered: c.covered, filled }, before);
      // Fremdes Fenster (z. B. nach shell.navigate) per Schließen-Knopf zu, damit die geprüfte App wieder vorn ist.
      for (let k = 0; k < 4; k++) {
        const other = await page.evaluate((app) => {
          const w = [...document.querySelectorAll('[data-window-app]')].find((x) => x.dataset.windowApp !== app && x.checkVisibility());
          if (!w) return null;
          const b = w.querySelector('[data-window-control="close"]');
          if (!b || !b.checkVisibility()) return { app: w.dataset.windowApp };
          const r = b.getBoundingClientRect();
          return { app: w.dataset.windowApp, x: r.left + r.width / 2, y: r.top + r.height / 2 };
        }, id);
        if (!other) break;
        steps[steps.length - 1].foreignWindow = (steps[steps.length - 1].foreignWindow || []).concat(other.app);
        if (other.x == null) break;
        await page.mouse.click(other.x, other.y);
        await settle(page, 400);
      }
      state = await page.evaluate(pageState);
      // Formular mit gesperrtem Speichern-Knopf (Pflichtfelder leer): Felder per Tastatur füllen, damit der Knopf frei
      // wird und der Speicherweg bedient werden kann.
      const lockedSave = await page.evaluate((src) => {
        const re = new RegExp(src, 'i');
        const root = document.querySelector('.v4-app-root');
        return [...(root?.querySelectorAll('button[disabled], [type=submit][disabled]') || [])].some((b) => b.checkVisibility() && re.test(b.getAttribute('aria-label') || b.textContent || ''));
      }, SAVE_RE.source);
      if (lockedSave) {
        const b1 = state;
        const f2 = await fillEmptyFields(page);
        await settle(page, 250);
        if (f2.length) state = await record({ label: 'Pflichtfelder gefüllt (Speichern war gesperrt)', role: 'fill', filled: f2 }, b1);
      }
      // Neu erschienene Elemente (Dialog, Schublade, Menü) zuerst – Tiefensuche in die geöffnete Fläche;
      // darin Eingaben/Reiter vor Speichern-Knöpfen, Abbrechen/Schließen zuletzt.
      const fresh = (await page.evaluate(pageCandidates)).filter((x) => !done.has(x.key) && !queue.includes(x.key) && !x.covered);
      const rank = (x) => (/^(abbrechen|cancel|schließen|close|×|✕|zurück|back)$/i.test(x.label) ? 3 : SAVE_RE.test(x.label) ? 2 : 1);
      fresh.sort((a, b) => rank(a) - rank(b));
      queue.unshift(...fresh.map((x) => x.key));
      // Schublade/Menü offen und nichts Neues mehr? Escape schließt Überlagerungen, damit der Rest erreichbar bleibt.

    }
    if (queue.length) steps.push({ label: `${queue.length} weitere Elemente nicht bedient (Grenze --max ${MAX})`, role: 'note', rest: queue.slice(0, 40) });
  } catch (e) {
    steps.push({ label: 'Lauf abgebrochen', problem: String(e.message || e).split('\n')[0].slice(0, 300) });
    flags.push({ step: steps.length - 1, label: 'Lauf abgebrochen', probs: [String(e.message || e).split('\n')[0].slice(0, 200)] });
    try { await shot('abbruch'); } catch {}
  }
  await ctx.close();
  writeFileSync(join(dir, `steps-${w}.json`), JSON.stringify({ app: id, width: w, steps, flags }, null, 1));
  return { app: id, width: w, steps: steps.length, clicks: steps.filter((s) => s.shot && !['launcher', 'open'].includes(s.role)).length, flags };
}

// Kontaktbogen je App×Breite: alle Schritt-Screenshots mit Beschriftung in ein PNG.
async function contactSheet(browser, base, id, w) {
  const dir = join(OUT, id);
  const data = JSON.parse(readFileSync(join(dir, `steps-${w}.json`), 'utf8'));
  const items = data.steps.filter((s) => s.shot);
  const esc = (s) => String(s ?? '').replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
  const cols = w <= 480 ? 6 : 4;
  const cw = w <= 480 ? 240 : 420;
  const html = `<!doctype html><meta charset="utf-8"><body style="margin:0;padding:6px;font:11px system-ui;background:#fff;width:${cols * (cw + 8)}px">
<div style="display:grid;grid-template-columns:repeat(${cols},${cw}px);gap:8px">${items.map((s, i) => `<figure style="margin:0"><img src="data:image/jpeg;base64,${readFileSync(join(dir, s.shot)).toString('base64')}" style="width:${cw}px;border:1px solid ${s.probs?.length ? '#d33' : '#bbb'}"><figcaption style="color:${s.probs?.length ? '#b11' : '#222'}">${i}. ${esc(s.label).slice(0, 50)} · ${esc(s.effect || '')}${s.log?.length ? ' · ' + esc(s.log.map((e) => e.kind === 'command' ? `cmd ${e.type}→${e.status}` : e.kind === 'write' ? `${e.op} ${e.collection}→${e.result || '?'}` : e.kind).join(', ')).slice(0, 120) : ''}${s.probs?.length ? ' · ' + esc(s.probs.join(', ')) : ''}</figcaption></figure>`).join('')}</div></body>`;
  const ctx = await browser.newContext({ viewport: { width: cols * (cw + 8) + 12, height: 400 }, deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  await page.setContent(html);
  await page.screenshot({ path: join(dir, `sheet-${w}.png`), fullPage: true });
  await ctx.close();
}

async function main() {
  mkdirSync(OUT, { recursive: true });
  const server = await serve();
  const base = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium.launch({ headless: true, ...(existsSync(EXE) ? { executablePath: EXE } : {}) });
  const summaryFile = join(OUT, 'summary.json');
  let summary = {};
  try { summary = JSON.parse(readFileSync(summaryFile, 'utf8')); } catch { /* neu */ }
  for (const id of APPS) {
    configs.set(id, { ...appConfig(id), title: OTHERS.find((o) => o.id === id)?.title });
    for (const w of WIDTHS) {
      const r = await runOne(browser, base, id, w);
      await contactSheet(browser, base, id, w).catch((e) => console.error('Kontaktbogen', id, w, e.message));
      summary[`${id}@${w}`] = { ...r, at: new Date().toISOString() };
      console.log(`${r.flags.length ? 'FLAG' : 'OK  '} ${String(w).padStart(4)} ${id.padEnd(18)} Schritte ${r.steps}, Klicks ${r.clicks}${r.flags.length ? ' · ' + r.flags.map((f) => `#${f.step} ${f.label.slice(0, 30)}: ${f.probs.join('/')}`).join(' | ').slice(0, 400) : ''}`);
    }
  }
  // erneut lesen und zusammenführen (parallele Läufe mit demselben --out überschreiben sich so nicht)
  try { summary = { ...JSON.parse(readFileSync(summaryFile, 'utf8')), ...summary }; } catch { /* neu */ }
  writeFileSync(summaryFile, JSON.stringify(summary, null, 1));
  await browser.close();
  server.close();
}

// Nur beim direkten Aufruf laufen; als Modul (Sonden, RUND2-Erweiterungen) liefert es serve/pageState/pageCandidates.
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main().catch((e) => { console.error(e); process.exit(1); });
export { serve, pageState, pageCandidates, OTHERS, chromium, EXE };
