// Icon + Wort (Owner-Regel workjet#801), shared/pane-icon-labels.js: Kurzwörter nur, wo Platz ist.
//
// Fälle an einem kanonischen Pane-Kopf (v4/frame/theme/theme.css + shared/base.css, ohne v4-Rahmen):
//   1 breit → Wörter sichtbar, keine Markierung
//   2 schmal → data-icons-only, Wörter ausgeblendet, kein Wort ragt aus seinem Knopf
//   3 wieder breit → Markierung weg, Wörter zurück
//   4 Titel wechselt ohne Größenänderung der Zeile und bräche mit Wörtern um → nur Icons, Titel einzeilig
//   5 dispose → Markierung entfernt, keine weitere Reaktion auf Größenänderung
// Gegenprobe: Fall 4 ist auf d671453c9 (Beobachter nur auf Zeilengröße) rot.
//
// Lauf: node src/apps/business-os/v4/frame/lab/icon-words.test.mjs
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { startServer } from './serve.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(path.join(here, '../../../package.json'));
const { chromium } = require('playwright');

const results = [];
const check = (name, ok, detail) => { results.push(!!ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` – ${detail}` : ''}`); };

const ICON = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M7 4v16l13-8z"/></svg>';
const header = (title) => `
  <header class="ctox-pane-header ctox-pane-band">
    <div class="ctox-pane-title-row">
      <div class="ctox-pane-titles"><h2 class="ctox-pane-title">${title}</h2></div>
      <div class="ctox-pane-actions">
        <button class="ctox-pane-icon" type="button" aria-label="Ausgabe erzeugen" data-short-de="Erzeugen" data-short-en="Generate">${ICON}</button>
        <button class="ctox-pane-icon" type="button" aria-label="Thema konfigurieren" data-short-de="Konfigurieren" data-short-en="Configure">${ICON}</button>
      </div>
    </div>
  </header>`;

const server = await startServer(0);
const base = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ headless: true });
const P = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
P.on('pageerror', (e) => errors.push(String(e)));
const LONG = 'Onboarding neue Mitarbeitende · Mi 14.10.';
try {
  await P.goto(`${base}/shared/base.css`);
  await P.setContent(`<!doctype html><html><head>
    <link rel="stylesheet" href="${base}/v4/frame/theme/theme.css">
    <link rel="stylesheet" href="${base}/shared/base.css">
    <style>body{margin:0;font:15px system-ui,sans-serif} .ctox-pane-title{font-size:15px;line-height:1.1;margin:0}
      #app{width:700px} #probe{position:absolute;left:-5000px;top:0}</style></head>
    <body><div id="app">${header('Themen')}</div><div id="probe"></div></body></html>`);
  await P.evaluate(async (url) => { window.__pil = await import(url); }, `${base}/shared/pane-icon-labels.js`);
  const settle = () => P.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 30)))));
  const state = () => P.evaluate(() => {
    const h = document.querySelector('#app .ctox-pane-header');
    const t = h.querySelector('.ctox-pane-title');
    const words = [...h.querySelectorAll('.ctox-pane-icon-label')];
    const shown = words.filter((w) => w.getBoundingClientRect().width > 0);
    const out = words.filter((w) => { const a = w.getBoundingClientRect(); const b = w.parentElement.getBoundingClientRect(); return a.width > 0 && (a.right > b.right + 0.5 || a.left < b.left - 0.5); });
    return { marked: h.hasAttribute('data-icons-only'), words: words.length, shown: shown.length, out: out.length, titleLines: Math.round(t.getBoundingClientRect().height / parseFloat(getComputedStyle(t).lineHeight)) };
  });
  const setWidth = (w) => P.evaluate((px) => { document.getElementById('app').style.width = `${px}px`; }, w);

  await P.evaluate(() => { window.__fit = window.__pil.applyPaneIconLabels(document.getElementById('app'), 'de'); });
  await settle();
  let s = await state();
  check('1 breit: Wörter sichtbar, keine Markierung', s.words === 2 && s.shown === 2 && !s.marked && s.out === 0, JSON.stringify(s));

  await setWidth(170);
  await settle();
  s = await state();
  check('2 schmal: nur Icons (data-icons-only), kein Wort ragt heraus', s.marked && s.shown === 0 && s.out === 0, JSON.stringify(s));

  await setWidth(700);
  await settle();
  s = await state();
  check('3 wieder breit: Wörter zurück', !s.marked && s.shown === 2, JSON.stringify(s));

  // Breite so wählen, dass der lange Titel mit Wörtern umbricht, ohne Wörter aber passt (Vorbedingung am Klon gemessen).
  const width = await P.evaluate((long) => {
    const probe = document.getElementById('probe');
    probe.innerHTML = document.getElementById('app').innerHTML;
    const h = probe.querySelector('.ctox-pane-header');
    h.removeAttribute('data-icons-only');
    h.querySelector('.ctox-pane-title').textContent = long;
    const lines = (w, iconsOnly) => {
      probe.style.width = `${w}px`;
      h.toggleAttribute('data-icons-only', iconsOnly);
      const t = h.querySelector('.ctox-pane-title');
      return Math.round(t.getBoundingClientRect().height / parseFloat(getComputedStyle(t).lineHeight));
    };
    for (let w = 900; w > 200; w -= 10) if (lines(w, false) > 1 && lines(w, true) === 1) return w;
    return 0;
  }, LONG);
  if (!width) check('4 Vorbedingung: Breite mit Umbruch nur durch Wörter', false, 'keine solche Breite gefunden');
  else {
    await setWidth(width);
    await settle();
    s = await state();
    const pre = !s.marked && s.shown === 2;
    await P.evaluate((long) => { document.querySelector('#app .ctox-pane-title').textContent = long; }, LONG);
    await settle();
    s = await state();
    check(`4 Titelwechsel ohne Größenänderung (Breite ${width}): nur Icons, Titel einzeilig`, pre && s.marked && s.titleLines === 1, JSON.stringify(s));
  }

  await P.evaluate(() => { window.__fit.dispose(); });
  await setWidth(170);
  await settle();
  s = await state();
  check('5 dispose: Markierung entfernt, keine Reaktion mehr', !s.marked, JSON.stringify(s));
  check('keine Seitenfehler', errors.length === 0, errors.join(' | '));
} finally {
  await browser.close();
  server.close();
}
const failed = results.filter((ok) => !ok).length;
console.log(`${results.length - failed}/${results.length} PASS`);
process.exitCode = failed ? 1 : 0;
