// Erzeugt v4/frame/shell-v2-desktop.css (Strang FR6d): Startmenü, Desktop und Crew-Leiste der Shell-V2 WÖRTLICH aus
// den Quellen, in der Reihenfolge, in der die Shell-V2 sie lädt:
//   ui-contract, app.css, mobile-host.css (index.html), modules/desktop/index.css (Desktop-App, beim Mount),
//   shared/business-chat.js installChatStyles (zur Laufzeit in <head> eingefügt; ${CREW_CREATURE_BASE_CSS} aus
//   shared/crew-renderer.js eingesetzt).
// Lauf: node src/apps/business-os/v4/frame/lab/extract-desktop-css.mjs
// Gleiches Verfahren wie extract-topbar-css.mjs (dessen Parser und Umschreibung, hier mit der Hülle [data-v4-v2]):
// Eine Regel kommt mit, wenn einer ihrer Selektoren ein Element der Probe trifft (Markup aus launcher.js, desktop.js,
// crew-bar.js); Zustände (Hover/Fokus, Pseudo-Elemente, .is-*, Attribute) zählen für den Treffer nicht.
// Zusätzlich mechanisch: [data-app="desktop"] → [data-v4-desktop-v2] (gleiche Spezifität; [data-app] gehört im v4-Rahmen
// dem Inhalts-root einer App).
import { createRequire } from 'node:module';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parse, splitList, ROOT_RE, scopeSelector as scopeWith, testSelector } from './extract-topbar-css.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const bos = path.resolve(here, '../../..');
const HULL = '[data-v4-v2]';
const OUT = path.join(bos, 'v4/frame/shell-v2-desktop.css');
const scopeSelector = (s) => scopeWith(s, HULL);

// CSS der Crew-Leiste: der Text, den installChatStyles() in <head> schreibt.
export async function chatCss() {
  const src = fs.readFileSync(path.join(bos, 'shared/business-chat.js'), 'utf8');
  const at = src.indexOf('function installChatStyles');
  const start = src.indexOf('style.textContent = `', at) + 'style.textContent = `'.length;
  const end = src.indexOf('\n  `;', start);
  const { CREW_CREATURE_BASE_CSS } = await import(pathToFileURL(path.join(bos, 'shared/crew-renderer.js')).href);
  const raw = src.slice(start, end);
  if (/\$\{(?!CREW_CREATURE_BASE_CSS\})/.test(raw)) throw new Error('installChatStyles enthält unbekannte Platzhalter');
  return raw.replace('${CREW_CREATURE_BASE_CSS}', CREW_CREATURE_BASE_CSS);
}
export const DESKTOP_REWRITE = (text) => text.replaceAll('[data-app="desktop"]', '[data-v4-desktop-v2]');

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const SOURCES = [
    ['ui-contract/v1/workjet-ui-contract.css', null],
    ['app.css', null],
    ['mobile-host.css', null],
    ['modules/desktop/index.css', DESKTOP_REWRITE],
    ['shared/business-chat.js#installChatStyles', null],
  ];
  const chat = await chatCss();
  const rules = SOURCES.flatMap(([f, rw]) => {
    const text = f.includes('#') ? chat : fs.readFileSync(path.join(bos, f), 'utf8');
    return parse(rw ? rw(text) : text, f);
  });

  const require = createRequire(path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
  const { chromium } = require('playwright');
  const exe = path.join(os.homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');
  const { startServer } = await import('./serve.mjs');
  const server = await startServer(0);
  const base = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium.launch({ executablePath: exe, headless: true });
  const page = await browser.newPage();
  await page.goto(`${base}/v4/frame/lab/index.html?extract=1`).catch(() => {});
  await page.setContent('<!doctype html><html lang="de"><head></head><body><div id="probe"></div></body></html>');
  await page.evaluate(async (b) => {
    const L = await import(`${b}/v4/frame/launcher.js`);
    const D = await import(`${b}/v4/frame/desktop.js`);
    const C = await import(`${b}/v4/frame/crew-bar.js`);
    const probe = document.getElementById('probe');
    probe.innerHTML = `
      <div class="v4f-start" data-v4-v2 data-shell-style="ctox"><div class="shell-start-menu-panel is-active">${L.startMenuMarkup()}</div></div>
      <div class="v4f-desktop" data-v4-v2 data-shell-style="ctox">${D.desktopMarkup()}</div>
      <div class="v4f-crew" data-v4-v2 data-shell-style="ctox"><div class="ctox-chat-root is-collapsed">${C.crewBarMarkup()}</div></div>`;
    const cat = document.createElement('div');
    cat.className = 'start-menu-category';
    cat.innerHTML = '<div class="start-menu-category-title"><span>A</span></div><div class="start-menu-item">' + L.startMenuItemMarkup() + '</div>';
    cat.querySelector('[data-v4-icon]').innerHTML = '<img alt="">';
    probe.querySelector('.start-menu-body').appendChild(cat);
    const acts = probe.querySelector('[data-v4-launcher-actions]');
    acts.innerHTML = '<button type="button" class="start-menu-footer-btn v4f-start-action"><span class="v4f-launcher-action-glyph"></span><span>N</span></button>';
    const icon = document.createElement('div');
    icon.className = 'desktop-icon';
    icon.innerHTML = D.desktopIconMarkup();
    icon.querySelector('[data-v4-icon]').innerHTML = '<img class="workjet-app-raster-icon" alt="">';
    probe.querySelector('.desktop-icons').appendChild(icon);
    const slot = document.createElement('span');
    slot.className = 'ctox-chat-crew-slot';
    slot.innerHTML = C.crewSlotMarkup();
    slot.firstElementChild.innerHTML = '<img alt="">';
    probe.querySelector('.ctox-chat-fab-creatures').appendChild(slot);
  }, base);

  const uniq = [...new Set(rules.filter((r) => r.kind === 'rule').flatMap((r) => splitList(r.selector)))];
  const matches = new Map();
  for (const theme of ['light', 'dark']) {
    const res = await page.evaluate(([list, theme]) => {
      document.documentElement.dataset.theme = theme;
      const probe = document.getElementById('probe');
      const hulls = [...probe.children];
      const out = {};
      for (const [scoped, test] of list) {
        if (!test) continue;
        try {
          const hit = [...document.querySelectorAll(test)].some((e) => hulls.some((h) => h === e || h.contains(e)));
          if (hit) out[scoped] = true;
        } catch { out[scoped] = 'invalid'; }
      }
      return out;
    }, [uniq.map((s) => [s, testSelector(scopeSelector(s))]), theme]);
    for (const [k, v] of Object.entries(res)) if (v === true) matches.set(k, true);
  }
  await browser.close();
  server.close();

  // ---------- Ausgabe (wie extract-topbar-css.mjs) ----------
  const BODY_RE = /^body((?:\[[^\]]*\]|\.[\w-]+|:[a-z-]+)*)$/;
  const isRootOnly = (s) => { if (BODY_RE.test(s)) return true; const m = ROOT_RE.exec(s); return !!m && !s.slice(m[0].length).trim(); };
  const tokenSel = (s) => (BODY_RE.test(s) ? (s === 'body' ? HULL : null) : scopeSelector(s));
  const kept = [];
  for (const r of rules) {
    if (r.kind !== 'rule') continue;
    const sels = splitList(r.selector);
    if (sels.every(isRootOnly)) continue;
    const hit = sels.filter((s) => matches.get(s));
    if (hit.length) kept.push({ ...r, sels: hit });
  }
  const used = new Set();
  const varRefs = (txt) => [...txt.matchAll(/var\(\s*(--[\w-]+)/g)].map((m) => m[1]);
  for (const r of kept) varRefs(r.body).forEach((v) => used.add(v));
  const rootRules = rules.filter((r) => r.kind === 'rule' && splitList(r.selector).every(isRootOnly) && splitList(r.selector).some((x) => tokenSel(x)));
  // Kommentare vor einer Deklaration gehören nicht zum Namen (sonst fehlen z. B. --elev-float/--elev-float-2 aus app.css).
  const declsOf = (body) => body.replace(/\/\*[\s\S]*?\*\//g, '').split(/;(?![^(]*\))/).map((d) => d.trim()).filter(Boolean).map((d) => { const k = d.indexOf(':'); return [d.slice(0, k).trim(), d.slice(k + 1).trim()]; });
  const TYPO = new Set(['font', 'font-family', 'font-size', 'line-height', 'color', 'letter-spacing', 'font-feature-settings', '-webkit-font-smoothing', '-moz-osx-font-smoothing', 'text-rendering', 'font-weight']);
  for (const r of rootRules) for (const [k, v] of declsOf(r.body)) if (TYPO.has(k) && splitList(r.selector).every((x) => /^(html|body|:root)$/.test(x))) varRefs(v).forEach((x) => used.add(x));
  let grew = true;
  while (grew) {
    grew = false;
    for (const r of rootRules) for (const [k, v] of declsOf(r.body)) if (k.startsWith('--') && used.has(k)) for (const x of varRefs(v)) if (!used.has(x)) { used.add(x); grew = true; }
  }
  const usedKeyframes = new Set(kept.flatMap((r) => [...r.body.matchAll(/animation(?:-name)?\s*:\s*([^;]+)/g)].flatMap((m) => m[1].split(/[\s,]+/))));
  const wrap = (ctx, inner) => ctx.reduceRight((acc, c) => `${c} {\n${acc}\n}`, inner);

  let css = `/* GENERIERT von v4/frame/lab/extract-desktop-css.mjs – nicht von Hand ändern (Strang FR6d, 15.10.2026).
   Shell-V2-Startmenü, -Desktop und -Crew-Leiste wörtlich aus ${SOURCES.map(([f]) => f).join(', ')}: Deklarationen,
   @media und Reihenfolge unverändert, Selektoren mechanisch auf die Hülle ${HULL} begrenzt (Spezifität gleich;
   [data-app="desktop"] → [data-v4-desktop-v2]). Änderungen nur in den Quellen, dann neu erzeugen; FR6d-G0 im
   Rahmen-Labor vergleicht beide Pfade pixelweise. */\n\n`;
  css += '/* --- Tokens aus :root/html der Quellen (transitiv benutzt), an der Hülle --- */\n';
  for (const r of rootRules) {
    const decls = declsOf(r.body).filter(([k]) => (k.startsWith('--') && used.has(k)) || (TYPO.has(k) && splitList(r.selector).every((x) => /^(html|body|:root)$/.test(x))));
    if (!decls.length) continue;
    const sel = [...new Set(splitList(r.selector).map(tokenSel).filter(Boolean))].join(',\n');
    css += `/* ${r.file} */\n${wrap(r.ctx, `${sel} {\n${decls.map(([k, v]) => `  ${k}: ${v};`).join('\n')}\n}`)}\n`;
  }
  css += '\n/* --- Regeln --- */\n';
  let lastFile = '';
  for (const r of kept) {
    if (r.file !== lastFile) { css += `\n/* ===== ${r.file} ===== */\n`; lastFile = r.file; }
    css += `${wrap(r.ctx, `${r.sels.map((x) => scopeSelector(x)).join(',\n')} {${r.body}}`)}\n`;
  }
  for (const k of rules.filter((r) => r.kind === 'keyframes' && usedKeyframes.has(r.name))) css += `${wrap(k.ctx, k.text)}\n`;
  fs.writeFileSync(OUT, css);
  console.log(`${kept.length} Regeln, ${used.size} Tokens, ${usedKeyframes.size} Keyframes → ${path.relative(bos, OUT)}`);
}
