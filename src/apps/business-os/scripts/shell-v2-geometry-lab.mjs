#!/usr/bin/env node
// Shell-V2-Geometrie-Labor: rendert jede App aus dem Arbeitsbaum in echtem
// V2-Fensterchrome (app.css/base.css), misst die Abnahmekriterien aus
// docs/business-os-shell-v2-contract.md §7 und schreibt Screenshots + Bericht.
// Kein Daemon, keine Anmeldung, keine Daten - reine Kopf-/Spaltengeometrie.
// Aufruf: node scripts/shell-v2-geometry-lab.mjs [--apps a,b] [--widths 1180,1000,720]
import http from 'node:http';
import { readFileSync, readdirSync, existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const OUT = process.argv.includes('--output-dir')
  ? path.resolve(process.argv[process.argv.indexOf('--output-dir') + 1])
  : path.join(ROOT, '../../..', 'output/playwright/shell-v2-geometry-lab');
mkdirSync(OUT, { recursive: true });

const argApps = process.argv.includes('--apps')
  ? process.argv[process.argv.indexOf('--apps') + 1].split(',')
  : null;
const WIDTHS = process.argv.includes('--widths')
  ? process.argv[process.argv.indexOf('--widths') + 1].split(',').map(Number)
  : [1180];
const WIDTH = WIDTHS[0];

function apps() {
  const list = [];
  // Kundenapps (installed-modules: rem-*, kundenpipeline, sellify) haben eigene
  // Shells und sind vom V2-Vertrag ausgenommen (Betreiber-Entscheid 31.08.2026).
  for (const base of ['modules']) {
    const dir = path.join(ROOT, base);
    if (!existsSync(dir)) continue;
    for (const id of readdirSync(dir)) {
      if (id === 'desktop' || id === 'kundenpipeline') continue;
      if (!existsSync(path.join(dir, id, 'index.html'))) continue;
      list.push({ id, base });
    }
  }
  return argApps ? list.filter((a) => argApps.includes(a.id)) : list;
}

const MIME = { '.css': 'text/css', '.js': 'text/javascript', '.mjs': 'text/javascript', '.html': 'text/html', '.svg': 'image/svg+xml', '.json': 'application/json', '.png': 'image/png', '.jpg': 'image/jpeg', '.woff2': 'font/woff2' };

function iconPalette(app) {
  // Gradient-Stops des App-Icons als Frame-Palette, wie sie die Shell zur
  // Laufzeit aus dem Icon ableitet - macht App-Akzente im Labor sichtbar.
  try {
    const svg = readFileSync(path.join(ROOT, app.base, app.id, 'icon.svg'), 'utf8');
    const stops = [...svg.matchAll(/stop-color="(#[0-9a-fA-F]{6})"/g)].map((m) => m[1]);
    if (!stops.length) return '';
    const start = stops[0];
    const end = stops[stops.length - 1] || start;
    return `--shell-v2-frame-start: ${start}; --shell-v2-frame-middle: ${stops[1] || end}; --shell-v2-frame-end: ${end}; --shell-v2-frame-top-joint: ${stops[1] || end}; --shell-v2-frame-left-joint: ${stops[1] || end}; --shell-v2-local-accent: ${stops[1] || end};`;
  } catch { return ''; }
}

function labHtml(app, width = WIDTH) {
  const frame = readFileSync(path.join(ROOT, app.base, app.id, 'index.html'), 'utf8');
  // Nur den Body-Inhalt des Modul-Frames uebernehmen.
  const body = frame.replace(/^[\s\S]*?<body[^>]*>/i, '').replace(/<\/body>[\s\S]*$/i, '')
    .replace(/<script[\s\S]*?<\/script>/gi, '');
  return `<!doctype html>
<html lang="de" data-theme="dark" data-shell-style="ctox">
<head><meta charset="utf-8">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/${app.base}/${app.id}/index.css">
<style>
  html,body{margin:0;height:100%;background:#0b0e13}
  .lab-desk{position:relative;width:100%;height:100vh;overflow:hidden}
</style></head>
<body>
<div class="lab-desk">
  <section class="shell-window is-focused" data-shell-window="true" data-shell-contract="v2"
    data-shell-window-chrome="shared-v2" data-shell-header-rows="2" data-shell-icon-rows="2"
    data-owner-id="desktop-app:${app.id}" data-app-mode="window"
    style="position:absolute;left:20px;top:20px;width:${width}px;height:760px;${iconPalette(app)}">
    <div class="shell-window-v2-icon"><img alt="" src="/${app.base}/${app.id}/icon.svg" onerror="this.remove()"></div>
    <header class="shell-window-header" data-window-header></header>
    <div class="shell-window-controls">
      <button class="shell-window-control shell-window-control--close" aria-label="Schliessen">×</button>
    </div>
    <div class="shell-window-content">
      <div class="module-root shell-window-module-root" data-module-root="${app.id}">
        <div class="shell-window-module-pane shell-window-module-pane--left"></div>
        <div class="shell-window-module-column-resizer shell-window-module-column-resizer--left"></div>
        <main class="module-content ${app.id}" data-module-content>
        ${body}
        </main>
        <div class="shell-window-module-column-resizer shell-window-module-column-resizer--right"></div>
        <div class="shell-window-module-pane shell-window-module-pane--right"></div>
      </div>
    </div>
  </section>
</div>
</body></html>`;
}

async function main() {
  const list = apps();
  const server = http.createServer((req, res) => {
    const u = decodeURIComponent(req.url.split('?')[0]);
    const m = u.match(/^\/__lab\/([a-z0-9-]+)\/([a-z0-9-]+)$/);
    if (m) {
      const app = list.find((a) => a.base === m[1] && a.id === m[2]);
      if (!app) { res.writeHead(404); return res.end(); }
      const w = Number(new URLSearchParams(req.url.split('?')[1] || '').get('w')) || WIDTH;
      res.writeHead(200, { 'content-type': 'text/html' });
      return res.end(labHtml(app, w));
    }
    const f = path.join(ROOT, u.slice(1));
    if (!f.startsWith(ROOT) || !existsSync(f)) { res.writeHead(404); return res.end(); }
    try {
      res.writeHead(200, { 'content-type': MIME[path.extname(f)] || 'application/octet-stream' });
      res.end(readFileSync(f));
    } catch { res.writeHead(500); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const port = server.address().port;

  const browser = await chromium.launch({ headless: true, ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH } : {}) });
  const page = await browser.newPage({ viewport: { width: WIDTH + 60, height: 820 } });
  const report = [];

  for (const app of list) {
   for (const w of WIDTHS) {
    await page.setViewportSize({ width: Math.max(w + 60, 780), height: 820 });
    await page.goto(`http://127.0.0.1:${port}/__lab/${app.base}/${app.id}?w=${w}`, { waitUntil: 'load' });
    await page.waitForTimeout(150);
    const r = await page.evaluate(() => {
      const w = document.querySelector('section.shell-window');
      const wr = w.getBoundingClientRect();
      const heads = [...w.querySelectorAll('.ctox-pane-header')]
        .filter((h) => { const r = h.getBoundingClientRect(); return r.width > 0 && r.height > 0; });
      const topRow = heads.filter((h) => h.getBoundingClientRect().top - wr.top < 46);
      const rowPx = Math.round(parseFloat(getComputedStyle(w).getPropertyValue('--shell-v2-header-row-size'))) || 37;
      const info = topRow.map((h) => {
        const cs = getComputedStyle(h), r = h.getBoundingClientRect();
        return { h: Math.round(r.height), left: Math.round(r.left - wr.left), right: Math.round(wr.right - r.right), padL: Math.round(parseFloat(cs.paddingLeft)), padR: Math.round(parseFloat(cs.paddingRight)) };
      });
      const first = info[0] || null, last = info[info.length - 1] || null;
      const heights = [...new Set(info.map((x) => x.h))];
      // Icon-Zone per Punktabtastung: was ist im Bereich des Icons (0..80 x 0..74)
      // tatsaechlich gemalt? Kanten-Tests scheitern an eingerueckten
      // Vollbreite-Zeilen; elementsFromPoint sieht nur echte Treffer.
      const intruderSet = new Set();
      for (const px of [12, 34, 56, 74]) {
        for (const py of [42, 56, 70]) {
          const top = document.elementsFromPoint(wr.left + px, wr.top + py)
            .find((e) => e.closest('.shell-window-content') && !e.matches('.shell-window-content'));
          if (!top) continue;
          if (top.closest('.ctox-pane-header')) continue;
          const cls = String(top.className || top.tagName);
          if (/module-root|module-content|module-loading-shell|layout|workspace|-module|-pane$|-editor$|scroll/.test(cls) === false || top.children.length === 0) {
            const cs2 = getComputedStyle(top);
            const painted = cs2.backgroundColor !== 'rgba(0, 0, 0, 0)' || top.textContent.trim().length > 0 || top.matches('img,svg,button,input');
            if (painted) intruderSet.add((cls.split(' ')[0] || top.tagName).slice(0, 30));
          }
        }
      }
      const intruders = [...intruderSet].slice(0, 5);
      // h5: jede sichtbare Spalte der obersten Reihe braucht ihren eigenen Kopf
      // ("alle header der spalten immer gleich gross" setzt voraus, dass es sie gibt).
      // h5: jede sichtbare Spalte der obersten Reihe braucht ihren eigenen Kopf.
      const moduleRoot = w.querySelector('.module-root');
      const paneKids = (el) => [...(el?.children || [])].filter((e) => {
        const r = e.getBoundingClientRect();
        return r.width > 120 && r.height > 200 && !/resizer/.test(String(e.className));
      });
      // Rahmen ist das Element, dessen Kinder die Spalten sind: erst module-root
      // selbst probieren (Panes koennen direkt darunter liegen, z.B. matching),
      // sonst das breiteste Kind mit mehreren Spaltenkindern.
      let frame = moduleRoot;
      if (paneKids(moduleRoot).length < 2) {
        const nested = [...(moduleRoot?.children || [])].find((c) => paneKids(c).length >= 2);
        if (nested) frame = nested;
        else if (paneKids(moduleRoot).length === 1) frame = moduleRoot;
        else frame = [...(moduleRoot?.children || [])].find((c) => c.getBoundingClientRect().width > 300) || moduleRoot;
      }
      const topPanes = paneKids(frame);
      const panesWithHead = topPanes.filter((p) => [...p.querySelectorAll('.ctox-pane-header')].some((h) => { const r = h.getBoundingClientRect(); return r.width > 0 && r.height > 0 && r.top - p.getBoundingClientRect().top < 50; })).length;
      const hasTitle = !!w.querySelector('.ctox-pane-header .ctox-pane-title');
      const hscroll = w.querySelector('.shell-window-content').scrollWidth > Math.round(wr.width) + 2;
      return {
        headers: info.length, heights, rowPx,
        h1: heights.length === 1 && heights[0] <= rowPx * 2 + 2,
        h2_iconClear: first ? (first.left + first.padL) >= 80 : false,
        h3_ctrlClear: last ? (last.right + last.padR) >= 74 : false,
        h4_titleSlot: hasTitle,
        h5_paneHeads: topPanes.length > 0 && panesWithHead === topPanes.length,
        panes: topPanes.length, panesWithHead,
        intruders, hscroll,
      };
    });
    const fails = ['h1', 'h2_iconClear', 'h3_ctrlClear', 'h4_titleSlot', 'h5_paneHeads'].filter((k) => !r[k]);
    if (r.intruders.length) fails.push('iconZone');
    if (r.hscroll) fails.push('hscroll');
    const shot = path.join(OUT, w === WIDTHS[0] ? `${app.id}.png` : `${app.id}-${w}.png`);
    await page.screenshot({ path: shot });
    report.push({ app: app.id, base: app.base, width: w, ok: fails.length === 0, fails, detail: r });
    process.stdout.write(`${fails.length === 0 ? 'OK  ' : 'FAIL'} ${String(w).padStart(4)} ${app.id.padEnd(28)} ${fails.join(',')}\n`);
   }
  }

  await browser.close(); server.close();
  writeFileSync(path.join(OUT, 'report.json'), JSON.stringify(report, null, 1));
  const gallery = `<!doctype html><meta charset="utf-8"><title>Shell-V2 Geometrie</title>
<body style="background:#0b0e13;color:#dde;font-family:system-ui;padding:16px">
<h1>Shell-V2 Geometrie · ${new Date().toISOString().slice(0, 16)} · ${report.filter((r) => r.ok).length}/${report.length} OK</h1>
${report.map((r) => `<div style="margin:18px 0"><h2 style="color:${r.ok ? '#7c9' : '#e77'}">${r.ok ? '✓' : '✗'} ${r.app} <small>${r.fails.join(', ')}</small></h2><img src="${r.app}.png" style="max-width:100%;border:1px solid #345"></div>`).join('')}
</body>`;
  writeFileSync(path.join(OUT, 'index.html'), gallery);
  console.log(`\n${report.filter((r) => r.ok).length}/${report.length} OK · Galerie: ${path.join(OUT, 'index.html')}`);
}
main().catch((e) => { console.error(e); process.exit(1); });
