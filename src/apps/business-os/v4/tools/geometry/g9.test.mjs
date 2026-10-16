// G9 Pixelmessung mit Gegenprobe (Strang PAL, M-17).
//
//   CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/637caf6 node --test v4/tools/geometry/g9.test.mjs
//
// Rahmen-Labor (stub-lab.html, gebundene Shell), 1280×800, Fenster frei (520×360). Apps:
//   notes          Lab-App, SVG-Icon mit runden, durchsichtigen Ecken (rx 14, einfarbig)
//   pal-gradient   SVG-Icon mit runden, durchsichtigen Ecken und Verlauf (wie Kunden-Apps mit SVG-Icon)
//   real-reports   Bugs & Features, vollflächiges Raster-Icon (modules/reports/assets/icon.jpg)
//   real-iot       IoT, vollflächiges Raster-Icon (modules/iot/assets/icon.jpg)
// Varianten: „neu“ = shared/window-manager.js des Arbeitsbaums, „alt“ = Stand OLD_REV (Kante über dem Halter-Hintergrund
// verrechnet, eingeführt mit 002b8ae5c). Gegenprobe: G9 meldet mit „alt“ die Naht bei Icons mit durchsichtigen Ecken,
// mit „neu“ nicht; die alte Eckpixel-Messung meldet sie in beiden Fällen nicht. Raster-Icons bleiben pixelgleich.
// Ausgaben: v4/tools/geometry/out/pal/<variante>-<app>.png (Fensterecke, 2×) und results-<variante>.json.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { startServer } from '../../frame/lab/serve.mjs';
import { g9Pixel, G9_MAX_DE } from './g9.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const BOS = path.resolve(HERE, '../../..');
const MOD = path.join(BOS, 'modules');
const OUT = path.join(HERE, 'out', 'pal');
const OLD_REV = process.env.PAL_OLD_REV || '51b796217';
const require = createRequire(path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
const { chromium } = require('playwright');
const EXE = path.join(os.homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');
const DPR = 2;
const GRADIENT_SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#0f766e"/><stop offset="1" stop-color="#2563eb"/></linearGradient></defs><rect width="64" height="64" rx="14" fill="url(#g)"/><circle cx="32" cy="32" r="11" fill="#fff"/></svg>';
const APPS = ['notes', 'pal-gradient', 'real-reports', 'real-iot'];

const oldWm = execFileSync('git', ['show', `${OLD_REV}:src/apps/business-os/shared/window-manager.js`], { cwd: BOS, maxBuffer: 1 << 26 });

async function measure(browser, base, variant) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: DPR, locale: 'de-DE' });
  const P = await ctx.newPage();
  const perr = [];
  P.on('pageerror', (e) => perr.push(String(e)));
  // PAL_NEW_WM=<datei>: Kandidat für „neu“ ohne Änderung am Arbeitsbaum messen.
  if (variant === 'neu' && process.env.PAL_NEW_WM) await P.route(/\/shared\/window-manager\.js(\?.*)?$/, (r) => r.fulfill({ status: 200, contentType: 'text/javascript', body: fs.readFileSync(process.env.PAL_NEW_WM) }));
  if (variant === 'alt') await P.route(/\/shared\/window-manager\.js(\?.*)?$/, (r) => r.fulfill({ status: 200, contentType: 'text/javascript', body: oldWm }));
  await P.route(/\/~a\/(?:real|pal)-[a-z-]+@/, (route) => {
    const m = /\/~a\/real-([a-z-]+)@[^/]+\/(.+)$/.exec(new URL(route.request().url()).pathname);
    if (m && /\.(jpg|png|svg)$/.test(m[2])) {
      const f = path.join(MOD, m[1], m[2]);
      if (fs.existsSync(f)) { route.fulfill({ status: 200, contentType: m[2].endsWith('.svg') ? 'image/svg+xml' : m[2].endsWith('.png') ? 'image/png' : 'image/jpeg', body: fs.readFileSync(f) }); return; }
      route.fulfill({ status: 404 }); return;
    }
    route.fulfill({ status: 200, contentType: 'text/javascript', body: 'export function mount(root){root.innerHTML="<div style=\\"padding:20px;font:13px system-ui\\">Inhalt</div>";return{unmount(){root.replaceChildren()}}}' });
  });
  await P.goto(`${base}/v4/frame/lab/stub-lab.html?shell=bound&inst=pal-${variant}-${Date.now().toString(36)}`);
  await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  await P.evaluate(async (svg) => {
    const add = (id, title, layout) => window.__lab.kernel.sim.publishAppVersion(id, '1.0.0', { title, layout: { category: 'Labor', default_width: 520, default_height: 360, ...layout }, styles: [] });
    await add('pal-gradient', 'Verlauf rund', { icon_svg: svg });
    await add('real-reports', 'Bugs & Features', { icon_asset: 'assets/icon.jpg' });
    await add('real-iot', 'IoT', { icon_asset: 'assets/icon.jpg' });
  }, GRADIENT_SVG);
  await P.waitForSelector('[data-v4-app="real-iot"]', { state: 'attached' });
  fs.mkdirSync(OUT, { recursive: true });
  const rows = {};
  for (const id of APPS) {
    const owner = await P.evaluate(async (id) => { const h = window.__lab.frame.shellHandle(); const o = h.openWindow(id, { geometry: { x: 200, y: 140, w: 520, h: 360 } }); await h.win(o).host.ready; return o; }, id);
    const sel = `.shell-window[data-window-app="${id}"]`;
    await P.waitForFunction((s) => { const w = document.querySelector(s); const i = w?.querySelector('[data-window-app-icon]'); return i && i.complete && i.naturalWidth > 0 && i.style.visibility !== 'hidden' && w.dataset.shellV2PaletteSource; }, sel, { timeout: 8000 }).catch(() => {});
    await P.waitForTimeout(900);
    const vars = await P.$eval(sel, (w) => { const s = getComputedStyle(w); return { src: w.dataset.shellV2PaletteSource || '', topJoint: s.getPropertyValue('--shell-v2-frame-top-joint').trim(), leftJoint: s.getPropertyValue('--shell-v2-frame-left-joint').trim() }; });
    const r = await g9Pixel(P, sel, { dpr: DPR, label: id });
    const wr = await P.$eval(sel, (w) => { const b = w.getBoundingClientRect(); return { l: b.left, t: b.top }; });
    const shot = path.join(OUT, `${variant}-${id}.png`);
    await P.screenshot({ path: shot, clip: { x: wr.l - 20, y: wr.t - 20, width: 240, height: 170 } });
    rows[id] = { ...vars, ...(r.row || {}), skip: r.skip || null, fails: r.fails, shot: path.relative(BOS, shot) };
    await P.evaluate((o) => window.__lab.frame.shellHandle().closeWindow(o), owner);
    await P.waitForTimeout(300);
  }
  await ctx.close();
  fs.writeFileSync(path.join(OUT, `results-${variant}.json`), `${JSON.stringify({ variant, oldRev: OLD_REV, maxDE: G9_MAX_DE, rows, pageErrors: perr }, null, 2)}\n`);
  return { rows, perr };
}

let server; let browser; let base;
const res = {};
test.before(async () => {
  server = await startServer(0);
  base = `http://127.0.0.1:${server.address().port}`;
  browser = await chromium.launch({ headless: true, ...(fs.existsSync(EXE) ? { executablePath: EXE } : {}) });
  res.alt = await measure(browser, base, 'alt');
  res.neu = await measure(browser, base, 'neu');
  for (const v of ['alt', 'neu']) {
    for (const [id, r] of Object.entries(res[v].rows)) console.log(`${v.padEnd(3)} ${id.padEnd(13)} G9 oben ΔE ${r.top} links ΔE ${r.left} | alter Eckpixel oben ΔE ${r.cornerTop} links ΔE ${r.cornerLeft} | Kante ${r.topJoint}/${r.leftJoint} (${r.src}) Logo ${JSON.stringify([r.logoTop, r.logoLeft])} Kante-Pixel ${JSON.stringify([r.edgeTop, r.edgeLeft])}`);
  }
});
test.after(async () => { await browser?.close(); server?.close(); });

test('G9 neu: alle Apps nahtlos (ΔE deckendes Logopixel ↔ Kante < 12), Palette aus dem Icon', () => {
  for (const id of APPS) {
    const r = res.neu.rows[id];
    assert.equal(r.skip, null, `${id}: ${r.skip}`);
    assert.ok(['icon', 'generated'].includes(r.src), `${id}: Quelle ${r.src}`);
    assert.deepEqual(r.fails, [], `${id}: ${JSON.stringify(r)}`);
  }
  assert.deepEqual(res.neu.perr, []);
});

test('Gegenprobe: mit dem alten Algorithmus meldet G9 die Naht bei Icons mit durchsichtigen Ecken (FAIL)', () => {
  for (const id of ['notes', 'pal-gradient']) {
    const r = res.alt.rows[id];
    assert.ok(r.fails.length >= 1 && r.top >= G9_MAX_DE && r.left >= G9_MAX_DE, `${id}: ${JSON.stringify(r)}`);
  }
});

test('Gegenprobe: die alte Eckpixel-Messung hätte die Naht übersehen (Eckpixel ↔ Kante < 12 trotz Naht)', () => {
  const r = res.alt.rows.notes;
  assert.ok(r.cornerTop < G9_MAX_DE && r.cornerLeft < G9_MAX_DE && r.top >= G9_MAX_DE, JSON.stringify(r));
});

test('Raster-Icons (Bugs & Features, IoT) sind vor und nach dem Fix pixelgleich', () => {
  for (const id of ['real-reports', 'real-iot']) {
    const a = res.alt.rows[id]; const n = res.neu.rows[id];
    assert.equal(n.topJoint, a.topJoint, `${id} Kantenfarbe oben`); assert.equal(n.leftJoint, a.leftJoint, `${id} Kantenfarbe links`);
    assert.deepEqual([n.edgeTop, n.edgeLeft], [a.edgeTop, a.edgeLeft], id);
    assert.deepEqual(a.fails, [], `${id} schon vorher nahtlos`);
  }
});

test('Neu: Kachel-Logos mit durchsichtigen Ecken bekommen den Halter in Logofarbe – keine dunklen Keile, auch der Eckpixel liegt < 12', () => {
  for (const id of ['notes', 'pal-gradient']) {
    const r = res.neu.rows[id];
    assert.ok(r.cornerTop < G9_MAX_DE && r.cornerLeft < G9_MAX_DE, `${id}: ${JSON.stringify(r)}`);
  }
});
