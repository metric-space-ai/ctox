#!/usr/bin/env node
// Shell v4 – Bedienrundgang (Strang RUND2), eigenständig neben rundgang.mjs (RUND1).
//
// Öffnet je App und Breite die App im echten v4-Rahmen über den Launcher (echter Mausklick), maximiert sie (1280),
// und bedient sie dann mit echter Maus: Reiter/Navigation, erste Listenzeile, alle Knöpfe (zerstörende zuletzt),
// Suche. Formulare, die nach einem Klick erscheinen, werden mit Testtext gefüllt und abgeschickt. Je Schritt:
// Schreibvorgänge/Befehle der App (api.put/patch/remove/command, mit lokalem Ergebnis), Zonenmeldungen,
// Seitenfehler, Geometrie (seitlicher Überlauf, Elemente außerhalb des Fensters, neue Kinder an document.body).
//
//   node v4/tools/rundgang/rundgang2.mjs --app notes,mail [--widths 1280,420] [--out <dir>] [--max-steps 28]
//
// Server/Seed wie das Geometrie-Labor (geometry-v4.mjs: resolveLabPath, labBinding, labBody, appConfig).
// Apps unter customer-modules/ werden zusätzlich aufgelöst (Fixture-Bau dann über fixtures.mjs mit umgebogenem
// Modulordner, ohne die Datei zu ändern).
// Ausgabe: <out>/<app>/<breite>-NN-<schritt>.png und <out>/<app>/<breite>.json, Konsole: Kurzbericht.
import { createServer } from 'node:http';
import { readFileSync, existsSync, statSync, mkdirSync, writeFileSync } from 'node:fs';
import { join, dirname, extname, normalize } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { appConfig, configs, resolveLabPath, labBinding, labBody } from '../geometry/geometry-v4.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '..', '..', '..'));
const CUSTOMER = join(BOS, 'customer-modules');
const args = process.argv.slice(2);
const opt = (name, dflt) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : dflt; };
const APPS = String(opt('--app', '')).split(',').filter(Boolean);
const WIDTHS = String(opt('--widths', '1280,420')).split(',').map(Number).filter(Boolean);
const OUT = normalize(opt('--out', join(HERE, 'shots', 'RUND2')));
const MAX_STEPS = Number(opt('--max-steps', '28'));
const vh = (w) => (w <= 480 ? 860 : 800);
const require = createRequire(join(homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const EXE = join(homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');

// ---------- Konfiguration (auch für customer-modules) ----------
async function customerConfig(id) {
  // fixtures.mjs mit Modulordner customer-modules laden (nur für den Fixture-Bau; Datei bleibt unverändert).
  const src = readFileSync(join(BOS, 'v4/tools/geometry/fixtures.mjs'), 'utf8')
    .replace(/const HERE = [^\n]*/, `const HERE = ${JSON.stringify(join(BOS, 'v4/tools/geometry'))};`)
    .replace(/const MODULES = join\(BOS, 'modules'\);/, `const MODULES = ${JSON.stringify(CUSTOMER)};`)
    .replace(/const sch = \(id\) => join\(MODULES, id, 'collections.schema.json'\);/,
      `const sch = (id) => { const c = join(MODULES, id, 'collections.schema.json'); return existsSync(c) ? c : join(${JSON.stringify(join(BOS, 'modules'))}, id, 'collections.schema.json'); };`);
  const tmp = join(OUT, `.fixtures-${id}.mjs`);
  mkdirSync(OUT, { recursive: true });
  writeFileSync(tmp, src);
  const fx = await import(pathToFileURL(tmp).href);
  const manifest = JSON.parse(readFileSync(join(BOS, 'v4/manifests', `${id}.manifest.json`), 'utf8'));
  const ml = manifest.layout || {};
  const layout = {};
  for (const k of ['icon_svg', 'icon_asset', 'category', 'default_width', 'default_height', 'min_width', 'min_height']) if (ml[k] != null) layout[k] = ml[k];
  const styles = Array.isArray(manifest.styles) ? manifest.styles : [];
  const { seed } = fx.buildFixtures(id);
  return { id, entry: manifest.entry || 'index.js', layout, styles, seed, allow: [] };
}
const isCustomer = (id) => !existsSync(join(BOS, 'modules', id)) && existsSync(join(CUSTOMER, id));

function resolvePath(p) {
  const a = /^\/~a\/([^/@]+)@([^/]+)\/(.+)$/.exec(p);
  if (a && isCustomer(a[1])) {
    const f = normalize(join(CUSTOMER, a[1], a[3]));
    if (f.startsWith(join(CUSTOMER, a[1])) && existsSync(f) && !statSync(f).isDirectory()) return f;
  }
  return resolveLabPath(p);
}

const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.woff2': 'font/woff2', '.wasm': 'application/wasm' };
const PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Shell v4 Rundgang</title><style>html,body{margin:0;height:100%}</style></head><body><div id="ctox-shell"></div>
<script type="module">
import { createStubKernel } from '/~stub/ctox-app-api-stub.mjs';
import * as frame from '/v4/frame/index.js';
import { createWindowHost } from '/v4/shell/app-host.js';
const app = new URLSearchParams(location.search).get('app');
const cfg = await (await fetch('/__rg/cfg/' + encodeURIComponent(app) + '.json')).json();
const rg = window.__rg = { app, ops: [], zone: [], titles: [], pushErrors: [] };
const kernel = createStubKernel({ instanceId: 'rg-' + app + '-' + Date.now(), userId: 'owner', storage: 'memory', latencyMs: 1, multiTab: false, locale: 'de-DE' });
await kernel.ready;
for (const [c, rows] of Object.entries(cfg.seed)) { try { await kernel.sim.push(c, rows); } catch (e) { rg.pushErrors.push(c + ': ' + e.message); } }
// Ab Stub 68b83a3 braucht jede Instanz-URL eine aktive Shell-Version (sonst „Keine aktive Shell-Version …“ beim
// Öffnen über den Launcher). Das Labor veröffentlicht deshalb die Shell selbst (Version „lab“, Einstieg v4/frame).
try { await kernel.sim.publishAppVersion('shell', 'lab', { entry: 'v4/frame/index.js' }); } catch (e) { rg.pushErrors.push('shell: ' + e.message); }
await kernel.sim.publishAppVersion(app, '1.0.0', { entry: cfg.entry, layout: cfg.layout, styles: cfg.styles });
await kernel.sim.settle();
const short = (v) => { try { return JSON.stringify(v).slice(0, 300); } catch { return String(v).slice(0, 300); } };
const wrapData = (data) => {
  const out = { ...data };
  for (const m of ['put', 'patch', 'remove', 'command']) {
    const orig = data[m];
    if (typeof orig !== 'function') continue;
    out[m] = function (...a) {
      const op = { m, coll: String(a[0]), id: m === 'put' ? a[1]?.id : (m === 'command' ? undefined : a[1]), body: short(m === 'patch' ? a[2] : a[1]), local: 'pending', result: null, at: Date.now() };
      rg.ops.push(op);
      let h;
      try { h = orig.apply(data, a); } catch (e) { op.local = 'throw: ' + (e.code || '') + ' ' + e.message; throw e; }
      h?.local?.then(() => { op.local = 'ok'; }, (e) => { op.local = 'rejected: ' + (e.code || '') + ' ' + (e.path || '') + ' ' + e.message; });
      h?.result?.then((r) => { op.result = r?.status + (r?.reason ? ':' + r.reason : ''); }, () => {});
      return h;
    };
  }
  if (data.status?.report) {
    const st = data.status;
    out.status = {
      ...st,
      report: (item) => { rg.zone.push({ kind: 'report', item: short(item), at: Date.now() }); return st.report(item); },
      clear: (code) => { rg.zone.push({ kind: 'clear', code, at: Date.now() }); return st.clear(code); },
    };
  }
  return out;
};
const wrapHost = (o) => {
  const api = o.api;
  const k = api.kernel;
  if (k && !k.__rgWrapped) {
    const orig = k.appApi.bind(k);
    k.appApi = (id, ver) => (id === app ? wrapData(orig(id, ver)) : orig(id, ver));
    k.__rgWrapped = true;
  }
  const sh = api.shell;
  const t = sh.setTitle;
  sh.setTitle = (x) => { rg.titles.push(String(x ?? '')); return t.call(sh, x); };
  return createWindowHost(o);
};
await kernel.sim.startShell({ startShell: (k, o = {}) => frame.startShell(k, { ...o, createWindowHost: wrapHost }), stopShell: () => frame.stopShell() });
rg.kernel = kernel; rg.frame = frame;
rg.bodyBefore = [...document.body.children];
window.__rgReady = true;
</script></body></html>`;

function serve() {
  const server = createServer((req, res) => {
    // App-URLs tragen seit Stub 68b83a3 die Shell-Bindung (/~a/<app>@<ver>~shell@lab/…). Die Labor-Auflösung
    // (geometry-v4.mjs: resolveLabPath/labBinding) kennt nur ungebundene App-URLs → Shell-Segment entfernen.
    const p = decodeURIComponent(new URL(req.url, 'http://rg').pathname).replace(/^(\/~a\/[^/]+?)~shell@[^/~]+/, '$1');
    if (p === '/__rg/page.html') { res.writeHead(200, { 'content-type': 'text/html', 'cache-control': 'no-store' }); res.end(PAGE); return; }
    const c = /^\/__rg\/cfg\/(.+)\.json$/.exec(p);
    if (c) { res.writeHead(200, { 'content-type': 'application/json' }); res.end(JSON.stringify(configs.get(c[1]) || {})); return; }
    const file = resolvePath(p);
    const binding = labBinding(p);
    if (!file || !binding) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream', 'cache-control': 'no-store' });
    res.end(labBody(file, undefined, binding));
  });
  return new Promise((r) => server.listen(0, '127.0.0.1', () => r(server)));
}

// ---------- In der Seite: Kandidaten, Zustand, Geometrie ----------
function pageProbe({ app }) {
  const roots = [...document.querySelectorAll('.v4-app-root')];
  const root = roots[0];
  if (!root) return { open: false };
  const win = root.closest('.shell-window') || root.parentElement;
  const W = win.getBoundingClientRect();
  const vis = (el) => el.checkVisibility?.({ checkOpacity: true, checkVisibilityCSS: true }) && el.getBoundingClientRect().width * el.getBoundingClientRect().height > 4;
  const label = (el) => (el.getAttribute('aria-label') || el.title || el.innerText || el.value || el.placeholder || '').replace(/\s+/g, ' ').trim().slice(0, 60);
  const sel = 'button, [role=tab], [role=button], a[href], summary, [role=menuitem], [role=option], [role=treeitem], tbody tr, [role=row], li[data-id], [data-id], [data-row-id], [data-record-id], [data-action], [data-view], [data-tab], select, input[type=checkbox]';
  const cands = [];
  const seen = new Set();
  for (const old of root.querySelectorAll('[data-rg-cand]')) delete old.dataset.rgCand;
  for (const el of root.querySelectorAll(sel)) {
    if (!vis(el) || el.disabled || el.getAttribute('aria-disabled') === 'true') continue;
    if (el.closest('[hidden]')) continue;
    const tag = el.tagName.toLowerCase();
    const role = el.getAttribute('role') || '';
    const kind = role === 'tab' || el.dataset.tab != null || el.dataset.view != null || (el.closest('nav,[role=tablist]') && tag !== 'select') ? 'nav'
      : (tag === 'tr' || role === 'row' || role === 'option' || role === 'treeitem' || ((el.dataset.id || el.dataset.rowId || el.dataset.recordId) && tag !== 'button')) ? 'row'
      : tag === 'select' ? 'select' : tag === 'input' ? 'check' : 'button';
    const l = label(el);
    if (kind === 'button' && !['button', 'a', 'summary'].includes(tag) && !role && !el.dataset.action) continue;
    const key = `${kind}|${tag}|${l}|${el.dataset.action || el.dataset.view || el.dataset.tab || ''}`;
    if (seen.has(key)) continue;
    seen.add(key);
    const r = el.getBoundingClientRect();
    el.dataset.rgCand = String(cands.length);
    cands.push({ idx: cands.length, key, kind, label: l, x: Math.round(r.left + Math.min(r.width / 2, 40)), y: Math.round(r.top + r.height / 2), destructive: /lösch|entfern|delete|remove|archiv|verwerf|zurücksetz|reset|abmeld|trennen|disconnect|stornier/i.test(l) });
  }
  // Klickbare Karten/Zeilen ohne Knopf-Semantik (cursor: pointer)
  for (const el of root.querySelectorAll('li, article, [class*="card"], [class*="row"], [class*="item"]')) {
    if (cands.length > 400) break;
    if (!vis(el) || el.closest('button') || el.querySelector('button, input, select, textarea') && el.children.length > 6) continue;
    if (getComputedStyle(el).cursor !== 'pointer' || (el.parentElement && getComputedStyle(el.parentElement).cursor === 'pointer')) continue;
    const l = label(el);
    const key = `row|${el.tagName.toLowerCase()}|${l}|`;
    if (seen.has(key)) continue;
    seen.add(key);
    const r = el.getBoundingClientRect();
    el.dataset.rgCand = String(cands.length);
    cands.push({ idx: cands.length, key, kind: 'row', label: l, x: Math.round(r.left + Math.min(r.width / 2, 40)), y: Math.round(r.top + r.height / 2), destructive: false });
  }
  // Geometrie
  const issues = [];
  if (root.scrollWidth > root.clientWidth + 1) issues.push(`root scrollt seitlich ${root.scrollWidth}>${root.clientWidth}`);
  let outCount = 0; let outEx = '';
  for (const el of root.querySelectorAll('*')) {
    if (!vis(el)) continue;
    const r = el.getBoundingClientRect();
    if (r.right > W.right + 2 || r.left < W.left - 2 || r.bottom > Math.max(W.bottom, innerHeight) + 2) {
      // nur zählen, wenn kein clippender Vorfahr innerhalb von root
      let p = el.parentElement; let clipped = false;
      while (p && p !== root.parentElement) { const cs = getComputedStyle(p); if (cs.overflowX !== 'visible' || cs.overflowY !== 'visible') { const pr = p.getBoundingClientRect(); if (pr.right <= W.right + 2 && pr.left >= W.left - 2) { clipped = true; break; } } p = p.parentElement; }
      if (!clipped) { outCount++; if (!outEx) outEx = `${el.tagName.toLowerCase()}.${String(el.className).split(' ')[0]} "${label(el).slice(0, 30)}" x ${Math.round(r.left)}..${Math.round(r.right)} y ${Math.round(r.top)}..${Math.round(r.bottom)} (Fenster x ${Math.round(W.left)}..${Math.round(W.right)} y ${Math.round(W.top)}..${Math.round(W.bottom)})`; }
    }
  }
  if (outCount) issues.push(`${outCount} Elemente außerhalb des Fensters, z. B. ${outEx}`);
  for (const sc of root.querySelectorAll('*')) {
    if (!vis(sc) || sc.closest('[data-scroll-x]')) continue;
    const cs = getComputedStyle(sc);
    if ((cs.overflowX === 'auto' || cs.overflowX === 'scroll') && sc.scrollWidth > sc.clientWidth + 2) { issues.push(`seitlicher Scroll in ${sc.tagName.toLowerCase()}.${String(sc.className).split(' ')[0]} ${sc.scrollWidth}>${sc.clientWidth}`); break; }
  }
  const before = new Set(window.__rg.bodyBefore);
  const foreign = [...document.body.children].filter((c) => !before.has(c) && vis(c) && !c.contains(win)).map((c) => `${c.tagName.toLowerCase()}.${String(c.className).split(' ')[0]}`);
  if (foreign.length) issues.push(`außerhalb des App-Fensters an body: ${foreign.join(', ')}`);
  const inputs = [...root.querySelectorAll('input:not([type=checkbox]):not([type=radio]):not([type=hidden]):not([type=file]), textarea, [contenteditable=true]')].filter(vis).length;
  const search = [...root.querySelectorAll('input[type=search], input[placeholder*="uch" i], input[aria-label*="uch" i], input[placeholder*="filter" i]')].filter(vis).length;
  const zoneEl = document.querySelector('[data-v4-status-zone], .v4-status-zone, [data-status-zone]');
  return {
    open: true, winMode: win.classList.contains('is-mobile-sheet') ? 'sheet' : (win.dataset.windowState || ''),
    win: { x: Math.round(W.left), w: Math.round(W.width), h: Math.round(W.height) },
    text: (root.innerText || '').length, html: root.innerHTML.length, cands, issues, inputs, search,
    ops: window.__rg.ops.length, zone: window.__rg.zone.length, dialogs: (window.__rg.kernel.sim.dialogs || []).length, title: window.__rg.titles.at(-1) || '',
    zoneText: zoneEl ? zoneEl.innerText.replace(/\s+/g, ' ').trim().slice(0, 200) : '',
  };
}

// Formular füllen: leere sichtbare Texteingaben im App-Root.
function fillForm() {
  const root = document.querySelector('.v4-app-root');
  const vis = (el) => el.checkVisibility?.({ checkVisibilityCSS: true }) && el.getBoundingClientRect().width > 4;
  const filled = [];
  for (const old of root.querySelectorAll('[data-rg-fill]')) delete old.dataset.rgFill;
  for (const el of root.querySelectorAll('input, textarea')) {
    if (!vis(el) || el.disabled || el.readOnly) continue;
    const t = (el.type || 'text').toLowerCase();
    if (['checkbox', 'radio', 'hidden', 'file', 'search', 'range', 'color', 'button', 'submit'].includes(t)) continue;
    if (el.value) continue;
    const v = t === 'number' ? '42' : t === 'email' ? 'rundgang@example.com' : t === 'date' ? '2026-10-20' : t === 'datetime-local' ? '2026-10-20T10:00' : t === 'time' ? '10:00' : t === 'url' ? 'https://example.com' : t === 'tel' ? '+49 30 1234567' : 'Rundgang Test';
    filled.push(el);
    el.dataset.rgFill = String(filled.length);
  }
  return filled.length;
}

// Kandidat per echter Maus: erst in den sichtbaren Bereich scrollen (wie ein Nutzer), dann Mitte anklicken.
async function clickCand(page, c) {
  const loc = page.locator(`[data-rg-cand="${c.idx}"]`).first();
  try { await loc.scrollIntoViewIfNeeded({ timeout: 1500 }); } catch { /* weg */ }
  const b = await loc.boundingBox({ timeout: 1000 }).catch(() => null);
  if (!b) return false;
  const x = b.x + Math.min(b.width / 2, 40), y = b.y + b.height / 2;
  // Verdeckt (Schublade/Dialog darüber)? Dann klickt ein Nutzer dort etwas anderes – nicht klicken.
  const hit = await page.evaluate(({ x, y, idx }) => { const t = document.querySelector(`[data-rg-cand="${idx}"]`); const e = document.elementFromPoint(x, y); return !!t && !!e && (t === e || t.contains(e) || e.contains(t) && e.tagName === 'LABEL'); }, { x, y, idx: c.idx });
  if (!hit) return false;
  await realClick(page, x, y);
  return true;
}

async function realClick(page, x, y) {
  await page.mouse.move(x, y);
  await page.mouse.click(x, y);
}

// ---------- Rundgang je App und Breite ----------
async function tour(browser, base, id, w) {
  const dir = join(OUT, id);
  mkdirSync(dir, { recursive: true });
  const ctx = await browser.newContext({ viewport: { width: w, height: vh(w) }, deviceScaleFactor: 1, locale: 'de-DE' });
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${String(e.message || e).slice(0, 220)}`));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(`console: ${m.text().slice(0, 220)}`); });
  page.on('response', (r) => { if (r.status() >= 400) errors.push(`HTTP ${r.status()} ${r.url().replace(base, '')}`); });
  const steps = [];
  let shotN = 0;
  const shot = async (name) => { const f = join(dir, `${w}-${String(++shotN).padStart(2, '0')}-${name.replace(/[^\w.-]+/g, '_').slice(0, 40)}.png`); await page.screenshot({ path: f }); return f.slice(OUT.length + 1); };
  const probe = () => page.evaluate(pageProbe, { app: id });
  const record = async (step, before, extra = {}) => {
    await page.waitForTimeout(450);
    const after = await probe();
    const ops = await page.evaluate((n) => window.__rg.ops.slice(n), before.ops ?? 0);
    const zone = await page.evaluate((n) => window.__rg.zone.slice(n), before.zone ?? 0);
    const dialogs = await page.evaluate((n) => (window.__rg.kernel.sim.dialogs || []).slice(n).map((d) => `${d.kind}: ${JSON.stringify(d.spec).slice(0, 120)}`), before.dialogs ?? 0);
    const errs = errors.splice(0);
    const changed = !after.open || after.html !== before.html || ops.length || zone.length || dialogs.length;
    const s = { step, changed, ops: ops.map((o) => `${o.m} ${o.coll}${o.id ? '/' + o.id : ''} local=${o.local} result=${o.result} ${o.body.slice(0, 140)}`), zone: zone.map((z) => (z.kind === 'clear' ? `clear ${z.code}` : z.item)), dialogs, errors: errs, issues: after.issues || [], open: after.open, ...extra };
    if (changed || errs.length || (after.issues || []).length) s.shot = await shot(step);
    steps.push(s);
    return after;
  };
  try {
    await page.goto(`${base}/__rg/page.html?app=${encodeURIComponent(id)}`);
    await page.waitForFunction(() => window.__rgReady === true, null, { timeout: 30000 });
    await page.waitForTimeout(300);
    // Desktop-Symbol per echter Maus (FR6d: das Startmenü ist ein Popup, die Fläche dahinter ist der Desktop)
    const tile = page.locator(`[data-v4-desk-app="${id}"]`).first();
    await tile.scrollIntoViewIfNeeded();
    const tb = await tile.boundingBox();
    if (!tb) throw new Error('Launcher-Kachel nicht sichtbar');
    await realClick(page, tb.x + tb.width / 2, tb.y + tb.height / 2);
    try { await page.waitForSelector('.v4-app-root', { timeout: 15000 }); } catch (e) {
      const diag = await page.evaluate(() => ({ apps: window.__rg.kernel.apps.get().map((x) => ({ id: x.id, state: x.state, active_ver: x.active_ver, error: x.error })), windows: document.querySelectorAll('.shell-window').length, tiles: document.querySelectorAll('[data-v4-app]').length, zone: (window.__rg.kernel.status?.get?.() || []).map((z) => z.code + ':' + z.text).slice(0, 5) }));
      throw new Error(`App-Fenster erscheint nicht: ${JSON.stringify(diag).slice(0, 600)}`);
    }
    await page.waitForTimeout(900);
    if (w > 480) {
      // Layout-Menü (echter Klick auf den Umschalter), dann „Maximieren“.
      for (const sel of ['[data-window-layout-control="toggle"]', '[data-window-layout-control="maximize"]']) {
        const b = await page.locator(sel).first().boundingBox({ timeout: 2000 }).catch(() => null);
        if (b) { await realClick(page, b.x + b.width / 2, b.y + b.height / 2); await page.waitForTimeout(350); }
      }
      await page.waitForTimeout(500);
    }
    let cur = await probe();
    // Schreibt die App schon beim bloßen Öffnen (Befund laut Orchestrator)?
    const openOps = await page.evaluate(() => window.__rg.ops.map((o) => `${o.m} ${o.coll}${o.id ? '/' + o.id : ''} local=${o.local} result=${o.result} ${o.body.slice(0, 120)}`));
    steps.push({ step: 'öffnen', ops: openOps, title: cur.title, win: cur.win, mode: cur.winMode, issues: cur.issues, errors: errors.splice(0), zoneText: cur.zoneText, cands: cur.cands.map((c) => `${c.kind}:${c.label.slice(0, 30)}`), shot: await shot('open') });
    const done = new Set();
    const order = (c) => (c.kind === 'nav' ? 0 : c.kind === 'row' ? 1 : c.kind === 'select' ? 3 : c.destructive ? 5 : 2);
    let rowsClicked = 0;
    const norm = (l) => String(l).replace(/[\d()]+/g, '').trim();
    const firstNav = cur.cands.find((c) => c.kind === 'nav');
    let navPhase = !!firstNav;
    for (let i = 0; i < MAX_STEPS; i++) {
      if (!cur.open) break;
      const next = cur.cands.filter((c) => !done.has(c.key) && !(c.kind === 'row' && rowsClicked >= 2) && c.kind !== 'check').sort((a, b) => order(a) - order(b))[0];
      if (!next) break;
      // Nach den Reitern zum ersten Reiter zurück (sonst bleibt ein leeres Band aktiv und Zeilen/Detail fehlen).
      if (navPhase && next.kind !== 'nav') {
        navPhase = false;
        const back = cur.cands.find((c) => c.kind === 'nav' && norm(c.label) === norm(firstNav.label));
        if (back) { const b0 = cur; await clickCand(page, back); cur = await record(`zurück zu Reiter ${back.label}`, b0, { kind: 'nav' }); continue; }
      }
      done.add(next.key);
      if (next.kind === 'row') rowsClicked++;
      if (next.kind === 'select') {
        const before = cur;
        const changedSel = await page.evaluate(({ idx }) => { const el = document.querySelector(`[data-rg-cand="${idx}"]`)?.closest('select'); if (!el || el.options.length < 2) return null; el.selectedIndex = (el.selectedIndex + 1) % el.options.length; el.dispatchEvent(new Event('input', { bubbles: true })); el.dispatchEvent(new Event('change', { bubbles: true })); return el.value; }, next);
        cur = await record(`auswahl ${next.label || '?'} → ${changedSel}`, before);
        continue;
      }
      const before = cur;
      if (!(await clickCand(page, next))) {
        // Verdeckt: wie ein Nutzer zuerst Escape, dann den Schließen-Knopf des obersten Dialogs/der Schublade.
        await page.keyboard.press('Escape');
        await page.waitForTimeout(250);
        let p2 = await probe();
        let again = p2.cands.find((c) => c.key === next.key);
        if (!again || !(await clickCand(page, again))) {
          const closed = await page.evaluate(() => {
            const root = document.querySelector('.v4-app-root');
            const layers = [...root.querySelectorAll('[role=dialog], dialog[open], .ctox-modal, [class*="modal"], [class*="drawer"], [data-spreadsheets-app-drawer]')].filter((e) => e.checkVisibility?.());
            const top = layers.at(-1);
            const b = top && [...top.querySelectorAll('button')].find((x) => /schließ|schliess|abbrech|close|^×$|^x$/i.test((x.getAttribute('aria-label') || x.title || x.innerText || '').trim()));
            if (b) { b.dataset.rgClose = '1'; return true; }
            return false;
          });
          if (closed) { const b = await page.locator('[data-rg-close="1"]').first().boundingBox({ timeout: 1000 }).catch(() => null); if (b) await realClick(page, b.x + b.width / 2, b.y + b.height / 2); await page.waitForTimeout(300); }
          p2 = await probe();
          again = p2.cands.find((c) => c.key === next.key);
          if (!again || !(await clickCand(page, again))) { cur = p2; steps.push({ step: `${next.kind} ${next.label || '?'}: verdeckt/nicht erreichbar, übersprungen` }); continue; }
        }
        steps.push({ step: `Overlay geschlossen (Escape/Schließen) vor ${next.label || '?'}` });
        cur = await record(`${next.kind} ${next.label || '?'}`, p2, { kind: next.kind });
        continue;
      }
      cur = await record(`${next.kind} ${next.label || '?'}`, before, { kind: next.kind });
      // Formular erschienen? füllen + abschicken
      if (cur.open && cur.inputs > before.inputs) {
        const n = await page.evaluate(fillForm);
        if (n) {
          for (let k = 1; k <= n; k++) {
            const loc = page.locator(`[data-rg-fill="${k}"]`);
            try { await loc.click({ timeout: 1500 }); await loc.fill(await loc.evaluate((el) => el.type === 'number' ? '42' : el.type === 'email' ? 'rundgang@example.com' : el.type === 'date' ? '2026-10-20' : el.type === 'datetime-local' ? '2026-10-20T10:00' : el.type === 'time' ? '10:00' : el.type === 'url' ? 'https://example.com' : el.type === 'tel' ? '+49 30 1234567' : 'Rundgang Test'), { timeout: 1500 }); } catch { /* egal */ }
          }
          const b2 = await probe();
          // Absendeknopf im selben Formular/Container wie das erste gefüllte Feld (sonst der letzte passende Knopf).
          const own = await page.evaluate((re) => {
            const f = document.querySelector('[data-rg-fill="1"]');
            const box = f?.closest('form, [role=dialog], dialog, aside, .ctox-drawer, section, article');
            if (!box) return null;
            const rx = new RegExp(re, 'i');
            const btns = [...box.querySelectorAll('button')].filter((b) => !b.disabled && b.checkVisibility?.());
            const b = btns.find((x) => x.type === 'submit' && box.tagName === 'FORM') || btns.reverse().find((x) => rx.test((x.getAttribute('aria-label') || x.innerText || '').trim()));
            return b?.dataset.rgCand ?? null;
          }, 'speicher|anlegen|erstell|hinzufüg|übernehm|senden|planen|buchen|zuweis|einreich|übergeb|hinterlass|save|create|^ok$|fertig|bestätig|hochladen|starten');
          const ownCand = own != null ? b2.cands.find((c) => String(c.idx) === String(own)) : null;
          const submit = ownCand || b2.cands.filter((c) => c.kind === 'button' && /speicher|anlegen|erstell|hinzufüg|übernehm|senden|planen|buchen|zuweis|einreich|übergeb|hinterlass|save|create|^ok$|fertig|bestätig|hochladen|starten/i.test(c.label)).at(-1);
          if (submit) {
            done.add(submit.key);
            await clickCand(page, submit);
            cur = await record(`formular ${n} Felder → ${submit.label}`, b2, { kind: 'submit' });
          } else {
            await page.keyboard.press('Enter');
            cur = await record(`formular ${n} Felder → Enter`, b2, { kind: 'submit' });
          }
        }
      }
      // Fenster weg? neu öffnen
      if (!cur.open) {
        const t2 = page.locator(`[data-v4-desk-app="${id}"]`).first();
        const b = await t2.boundingBox({ timeout: 2000 }).catch(() => null);
        if (b) { await realClick(page, b.x + b.width / 2, b.y + b.height / 2); await page.waitForTimeout(900); cur = await probe(); steps.push({ step: 'wieder öffnen (Fenster war zu)', open: cur.open }); }
      }
    }
    // Suche
    const sb = page.locator('.v4-app-root input[type=search], .v4-app-root input[placeholder*="uch" i]').first();
    if (await sb.count() && await sb.isVisible().catch(() => false)) {
      const before = await probe();
      // Echter Klick; ist das Feld verdeckt (offene Schublade/Overlay), ist das selbst ein Befund.
      try { await sb.click({ timeout: 3000 }); } catch (e) { steps.push({ step: 'suche: Feld nicht klickbar (verdeckt?)', errors: [String(e.message).split('\n').slice(0, 3).join(' ').slice(0, 300)], shot: await shot('suche-verdeckt') }); }
      await sb.fill('zzzz-kein-treffer', { timeout: 3000 }).catch(() => {});
      const mid = await record('suche ohne Treffer', before, { kind: 'search', textBefore: before.text });
      steps.at(-1).textAfter = mid.text;
      await sb.fill('', { timeout: 3000 }).catch(() => {});
      await record('suche geleert', mid, { kind: 'search' });
    }
    const opsAll = await page.evaluate(() => window.__rg.ops.map((o) => ({ m: o.m, coll: o.coll, local: o.local, result: o.result })));
    const pushErrors = await page.evaluate(() => window.__rg.pushErrors);
    const res = { app: id, width: w, steps, opsAll, pushErrors, titles: await page.evaluate(() => window.__rg.titles) };
    writeFileSync(join(dir, `${w}.json`), JSON.stringify(res, null, 1));
    await ctx.close();
    return res;
  } catch (e) {
    steps.push({ step: 'ABBRUCH', error: String(e.message || e).split('\n')[0], errors: errors.splice(0) });
    try { steps.at(-1).shot = await shot('abbruch'); } catch { /* egal */ }
    const res = { app: id, width: w, steps, aborted: true };
    writeFileSync(join(dir, `${w}.json`), JSON.stringify(res, null, 1));
    await ctx.close();
    return res;
  }
}

// Gezielte Fixture-Korrekturen je App, damit der Rundgang echte Inhalte bedienen kann (Fixture-Generator kennt die
// fachlichen Schlüssel nicht). Nur Seed des Rundgangs; fixtures.mjs bleibt unverändert.
const SEED_PATCH = {
  mail(seed) {
    const acc = seed.communication_accounts || [];
    acc.forEach((a) => { a.channel = 'email'; a.profile_json = { ...(typeof a.profile_json === 'object' ? a.profile_json : {}), owner_user_id: 'owner' }; });
    const ids = acc.map((a) => a.id);
    for (const c of ['communication_threads', 'communication_messages']) (seed[c] || []).forEach((r, i) => { r.channel = 'email'; if (ids.length) r.account_key = ids[i % ids.length]; });
  },
  matching(seed) {
    (seed.matching_requirements || []).forEach((r) => { r.kind = 'requirement'; });
    (seed.matching_objects || []).forEach((r) => { r.kind = 'object'; });
    (seed.matching_results || []).forEach((r) => { r.kind = 'match'; });
  },
};

async function main() {
  mkdirSync(OUT, { recursive: true });
  const server = await serve();
  const base = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium.launch({ headless: true, ...(existsSync(EXE) ? { executablePath: EXE } : {}) });
  for (const id of APPS) {
    const cfg = isCustomer(id) ? await customerConfig(id) : appConfig(id);
    // fixtures.mjs:128 erkennt `_deleted` nicht (führender Unterstrich) und setzt es für jede zweite Zeile auf true:
    // diese Zeilen wären Grabsteine. Für den Rundgang entfernen (Befund an GEO gemeldet, Datei nicht geändert).
    for (const rows of Object.values(cfg.seed)) for (const r of rows) delete r._deleted;
    // Fixture-Nutzer „user-001“ ist im Stub nicht der angemeldete Nutzer („owner“): Zuordnungsfelder der ersten
    // Fixture-Person auf owner umbiegen, damit Apps mit „meine“-Filtern (threads, support …) Daten zeigen.
    for (const rows of Object.values(cfg.seed)) for (const r of rows) for (const k of Object.keys(r)) {
      if (r[k] === 'user-001' && /(^|_)(user_id|owner_user_id|assigned_user_id|assignee_id|owner_id|author_user_id|actor_user_id|recipient_user_id)$/.test(k)) r[k] = 'owner';
    }
    (SEED_PATCH[id] || (() => {}))(cfg.seed);
    configs.set(id, cfg);
    for (const w of WIDTHS) {
      const r = await tour(browser, base, id, w);
      const writes = (r.opsAll || []);
      const bad = writes.filter((o) => o.local !== 'ok' || (o.result && !/^accepted/.test(o.result)));
      const errs = r.steps.flatMap((s) => s.errors || []);
      const iss = [...new Set(r.steps.flatMap((s) => s.issues || []))];
      console.log(`${r.aborted ? 'ABBR' : 'OK  '} ${id} ${w}: ${r.steps.length} Schritte, ${writes.length} Schreib/Befehle (${bad.length} abgelehnt/offen), ${errs.length} Fehler, ${iss.length} Geometrie`);
      for (const b of bad.slice(0, 6)) console.log(`   ✗ ${b.m} ${b.coll} local=${b.local} result=${b.result}`);
      for (const e of [...new Set(errs)].slice(0, 6)) console.log(`   ! ${e}`);
      for (const g of iss.slice(0, 4)) console.log(`   ⟷ ${g}`);
      if (r.aborted) console.log(`   ABBRUCH: ${r.steps.at(-1).error}`);
    }
  }
  await browser.close();
  server.close();
}
main().catch((e) => { console.error(e); process.exit(1); });
