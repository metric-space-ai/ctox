// Erzeugt shared/shell-v2-topbar.css: die Kopfleisten-Regeln der Shell-V2 WÖRTLICH aus den Stylesheets, die
// index.html lädt (ui-contract, app.css, mobile-host.css, shared/shell-topbar.css), in Quellreihenfolge.
// Lauf: node src/apps/business-os/v4/frame/lab/extract-topbar-css.mjs   (Strang TOPBAR; Änderungen nur in den Quellen)
//
// Auswahl: Eine Regel kommt mit, wenn einer ihrer Selektoren ein Element der Kopfleiste (topbar.js topbarMarkup, hell
// und dunkel) trifft. Für den Treffer zählen Zustände nicht (Hover/Fokus, ::before/::after, Zustandsattribute wie
// [open], [aria-current], [data-sync-state] und .is-*), damit alle Zustandsregeln mitkommen.
// Übernommen werden nur die treffenden Selektoren einer Liste; Deklarationen, @media/@supports und Reihenfolge bleiben
// Zeichen für Zeichen. Mechanisch umgeschrieben (Spezifität bleibt gleich):
//   X                                   → :where([data-v4-topbar]) X     (Kopfleiste ist nicht mehr global, sonst
//                                                                          träfen .drawer-body/.text-button die Apps)
//   :root[data-theme=…] X               → :root[data-theme=…] :where([data-v4-topbar]) X
//   :root[data-shell-style=…] X         → [data-v4-topbar][data-shell-style=…] X          (Stil hängt an der Hülle)
//   :root[data-shell-style=…][data-theme=…] X → :root[data-theme=…] :where([data-v4-topbar])[data-shell-style=…] X
// Tokens: alle von den Regeln (transitiv) benutzten Custom Properties aus den :root/html-Blöcken, an der Hülle gesetzt;
// dazu die Schrift aus html/body. @keyframes, die eine Regel nennt, kommen mit.
import { createRequire } from 'node:module';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { startServer } from './serve.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const bos = path.resolve(here, '../../..');
const SOURCES = ['ui-contract/v1/workjet-ui-contract.css', 'app.css', 'mobile-host.css', 'shared/shell-topbar.css'];
const OUT = path.join(bos, 'shared/shell-v2-topbar.css');

// ---------- Parser: Regeln mit Originaltext und At-Kontext ----------
function parse(text, file) {
  const out = [];
  let i = 0;
  const n = text.length;
  function skipWsComments() {
    for (;;) {
      while (i < n && /\s/.test(text[i])) i++;
      if (text.startsWith('/*', i)) { const e = text.indexOf('*/', i + 2); i = e < 0 ? n : e + 2; continue; }
      return;
    }
  }
  function readUntil(chars) { // bis { ; oder }, Strings/Klammern/Kommentare beachten
    let depth = 0; const start = i;
    while (i < n) {
      const c = text[i];
      if (text.startsWith('/*', i)) { const e = text.indexOf('*/', i + 2); i = e < 0 ? n : e + 2; continue; }
      if (c === '"' || c === "'") { i++; while (i < n && text[i] !== c) { if (text[i] === '\\') i++; i++; } i++; continue; }
      if (c === '(' || c === '[') depth++;
      else if (c === ')' || c === ']') depth--;
      else if (depth === 0 && chars.includes(c)) break;
      i++;
    }
    return text.slice(start, i);
  }
  function block() { // i steht auf '{' → Inhalt bis passendes '}'
    const start = ++i; let depth = 1;
    while (i < n && depth) {
      const c = text[i];
      if (text.startsWith('/*', i)) { const e = text.indexOf('*/', i + 2); i = e < 0 ? n : e + 2; continue; }
      if (c === '"' || c === "'") { i++; while (i < n && text[i] !== c) { if (text[i] === '\\') i++; i++; } i++; continue; }
      if (c === '{') depth++; else if (c === '}') depth--;
      i++;
    }
    return text.slice(start, i - 1);
  }
  function level(ctx) {
    for (;;) {
      skipWsComments();
      if (i >= n || text[i] === '}') { i++; return; }
      const prelude = readUntil('{;}').trim();
      if (text[i] === ';') { i++; continue; }
      if (text[i] !== '{') { i++; continue; }
      if (/^@(media|supports|container|layer)\b/i.test(prelude)) { i++; level([...ctx, prelude]); continue; }
      const body = block();
      if (/^@keyframes\s+/i.test(prelude)) { out.push({ kind: 'keyframes', name: prelude.split(/\s+/)[1], text: `${prelude} {${body}}`, ctx, file }); continue; }
      if (prelude.startsWith('@')) continue;
      out.push({ kind: 'rule', selector: prelude, body, ctx, file });
    }
  }
  level([]);
  return out;
}

function splitList(sel) {
  const parts = []; let depth = 0; let cur = '';
  for (const c of sel) {
    if (c === '(' || c === '[') depth++;
    if (c === ')' || c === ']') depth--;
    if (c === ',' && depth === 0) { parts.push(cur.trim()); cur = ''; } else cur += c;
  }
  if (cur.trim()) parts.push(cur.trim());
  return parts.map((s) => s.replace(/\s+/g, ' '));
}

// FR6d: :root:not([…]) (z. B. :root:not([data-theme="light"])) bleibt an der Wurzel, statt an die Hülle zu wandern.
const ROOT_RE = /^(:root|html)((?:\[[^\]]*\]|:not\(\[[^\]]*\]\)|:(?!where|is|not|has)[a-z-]+)*)(?=\s|>|$)/;
// Selektor → Kopfleisten-Selektor (siehe Kopf). null = reine Wurzelregel (Tokens).
// FR6d: Hülle als Parameter (Standard [data-v4-topbar]); extract-desktop-css.mjs nutzt dieselben Regeln für [data-v4-v2].
export function scopeSelector(s, hull = '[data-v4-topbar]') {
  const m = ROOT_RE.exec(s);
  if (!m) {
    if (/^body\b/.test(s)) {
      const b = /^body((?:\[[^\]]*\]|\.[\w-]+|:[a-z-]+)*)/.exec(s)[0];
      return `${b} :where(${hull})${s.slice(b.length)}`;
    }
    return `:where(${hull}) ${s}`;
  }
  const attrs = m[2].match(/:not\(\[[^\]]*\]\)|\[[^\]]*\]|:[a-z-]+/g) || [];
  const shell = attrs.filter((a) => a.startsWith('[data-shell-style')).join('');
  const other = attrs.filter((a) => !a.startsWith('[data-shell-style')).join('');
  const rest = s.slice(m[0].length);
  if (!rest.trim() && !shell) return other ? `:root${other} :where(${hull})` : hull;
  if (shell) return other ? `:root${other} :where(${hull})${shell}${rest}` : `${hull}${shell}${rest}`;
  return `:root${other} :where(${hull})${rest}`;
}
const KEEP_ATTR = /^\[(data-v4-topbar|data-v4-v2|data-shell-style|data-theme)/;
const DYN = /::?(?:hover|focus-visible|focus-within|focus|active|visited|checked|disabled|enabled|placeholder-shown|indeterminate|popover-open|before|after|placeholder|marker|selection|backdrop|first-letter|first-line|-webkit-[a-z-]+|-moz-[a-z-]+)(?![\w-])/g;
export function testSelector(s) {
  let t = s.replace(DYN, '');
  t = t.replace(/\[[^\]]*\]/g, (a) => (KEEP_ATTR.test(a) ? a : ''));
  t = t.replace(/\.is-[\w-]+/g, '');
  t = t.replace(/:not\(\s*\)/g, '').replace(/\(\s*\)/g, '');
  return t.trim() || null;
}

// Als Bibliothek importiert (extract-desktop-css.mjs): nur die Funktionen oben, kein Lauf.
export { parse, splitList, ROOT_RE };
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
// ---------- Matching im Browser ----------
const require = createRequire(path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
const { chromium } = require('playwright');
const exe = path.join(os.homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');

const rules = SOURCES.flatMap((f) => parse(fs.readFileSync(path.join(bos, f), 'utf8'), f));
const server = await startServer(0);
const base = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ executablePath: exe, headless: true });
const page = await browser.newPage();
await page.goto(`${base}/v4/frame/lab/index.html?extract=1`).catch(() => {});
await page.setContent('<!doctype html><html lang="de"><head></head><body><div id="probe"></div></body></html>');
await page.evaluate(async (url) => {
  const { topbarMarkup } = await import(url);
  const probe = document.getElementById('probe');
  probe.outerHTML = `<div class="v4f-top" data-v4-topbar data-shell-style="ctox" id="probe">${topbarMarkup()}</div>`;
  const p = document.getElementById('probe');
  // Beispielreiter + Überlauf wie topbar.js (Icon mit Rasterbild)
  p.querySelector('[data-module-tabs]').innerHTML = '<button class="module-tab" type="button" data-module data-target="a" data-target-kind="app"><span class="module-tab-icon"><img class="workjet-app-raster-icon" alt=""></span><span class="module-tab-label">A</span></button><details class="module-overflow"><summary class="module-overflow-trigger">+1</summary><div class="module-overflow-menu"><button class="module-tab" type="button"><span class="module-tab-icon"></span><span class="module-tab-label">B</span></button></div></details><div class="module-tabs-measure"></div>';
}, `${base}/v4/frame/topbar.js`);

const tested = [];
for (const r of rules) if (r.kind === 'rule') for (const s of splitList(r.selector)) tested.push(s);
const uniq = [...new Set(tested)];
const matches = new Map();
for (const theme of ['light', 'dark']) {
  const res = await page.evaluate(([list, theme]) => {
    document.documentElement.dataset.theme = theme;
    const probe = document.getElementById('probe');
    const out = {};
    for (const [scoped, test] of list) {
      if (!test) continue;
      try {
        const shell = probe.querySelector(':scope > .app-shell');
        const hit = [...document.querySelectorAll(test)].some((e) => e !== shell && (e === probe || probe.contains(e)));
        if (hit) out[scoped] = true;
      } catch { out[scoped] = 'invalid'; }
    }
    return out;
  }, [uniq.map((s) => { const sc = scopeSelector(s); return [s, testSelector(sc)]; }), theme]);
  for (const [k, v] of Object.entries(res)) if (v === true) matches.set(k, true);
}
await browser.close();
server.close();

// ---------- Ausgabe ----------
const BODY_RE = /^body((?:\[[^\]]*\]|\.[\w-]+|:[a-z-]+)*)$/;
// Reine Wurzelregeln (:root/html/body ohne Nachfahren) sind keine Kopfleistenregeln: nur Tokens/Schrift daraus.
const isRootOnly = (s) => { if (BODY_RE.test(s)) return true; const m = ROOT_RE.exec(s); return !!m && !s.slice(m[0].length).trim(); };
// Für den Tokenblock gelten nur unbedingte body-Regeln (body[data-…]/body.is-… sind Zustände der Shell-V2-Seite).
const tokenSel = (s) => (BODY_RE.test(s) ? (s === 'body' ? '[data-v4-topbar]' : null) : scopeSelector(s));
const kept = [];
for (const r of rules) {
  if (r.kind !== 'rule') continue;
  const sels = splitList(r.selector);
  if (sels.every(isRootOnly)) continue; // Tokens separat
  const hit = sels.filter((s) => matches.get(s));
  if (hit.length) kept.push({ ...r, sels: hit });
}
// Tokens transitiv
const used = new Set();
const varRefs = (txt) => [...txt.matchAll(/var\(\s*(--[\w-]+)/g)].map((m) => m[1]);
for (const r of kept) varRefs(r.body).forEach((v) => used.add(v));
const rootRules = rules.filter((r) => r.kind === 'rule' && splitList(r.selector).every(isRootOnly) && splitList(r.selector).some((x) => tokenSel(x)));
const declsOf = (body) => body.split(/;(?![^(]*\))/).map((d) => d.trim()).filter(Boolean).map((d) => { const k = d.indexOf(':'); return [d.slice(0, k).trim(), d.slice(k + 1).trim()]; });
const TYPO = new Set(['font', 'font-family', 'font-size', 'line-height', 'color', 'letter-spacing', 'font-feature-settings', '-webkit-font-smoothing', '-moz-osx-font-smoothing', 'text-rendering', 'font-weight']);
for (const r of rootRules) for (const [k, v] of declsOf(r.body)) if (TYPO.has(k) && splitList(r.selector).every((x) => /^(html|body|:root)$/.test(x))) varRefs(v).forEach((x) => used.add(x));
let grew = true;
while (grew) {
  grew = false;
  for (const r of rootRules) for (const [k, v] of declsOf(r.body)) if (k.startsWith('--') && used.has(k)) for (const x of varRefs(v)) if (!used.has(x)) { used.add(x); grew = true; }
}
const usedKeyframes = new Set(kept.flatMap((r) => [...r.body.matchAll(/animation(?:-name)?\s*:\s*([^;]+)/g)].flatMap((m) => m[1].split(/[\s,]+/))));
const wrap = (ctx, inner) => ctx.reduceRight((acc, c) => `${c} {\n${acc}\n}`, inner);

let css = `/* GENERIERT von v4/frame/lab/extract-topbar-css.mjs – nicht von Hand ändern (Strang TOPBAR, 15.10.2026).
   Shell-V2-Kopfleiste wörtlich aus ${SOURCES.join(', ')}: Deklarationen, @media und Reihenfolge unverändert,
   Selektoren mechanisch auf die Hülle [data-v4-topbar] begrenzt (Spezifität gleich, Regeln im Kopf des Skripts).
   Änderungen nur in den Quellen, dann neu erzeugen; G0 „TOP“ im Rahmen-Labor vergleicht beide Pfade pixelweise. */\n\n`;
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
css += `
/* --- v4-Anpassung (nicht aus app.css): Kopfleiste als erste Zeile des Rahmen-Rasters, Fehlerfarbe des Punkts,
       Einstellungseintrag oben im Konto-Blatt. --- */
.v4f[data-v4-has-topbar] { grid-template-rows: var(--shell-topbar-height, 48px) minmax(0, 1fr) var(--v4f-bar-h); }
.v4f-top { position: relative; z-index: 60; min-width: 0; line-height: normal; } /* .v4f vererbt 13px/1.35, Shell-V2-body: normal */
.v4f-top > .app-shell { display: block; height: 100%; min-width: 0; }
.v4f-top .shell-sync-status[data-sync-state="error"] .shell-sync-dot { background: var(--danger); }
.v4f-top .shell-sync-status[data-sync-state="error"] { color: var(--danger); }
.v4f-top .account-settings-entry { align-self: flex-start; }
`;
fs.writeFileSync(OUT, css);
console.log(`${kept.length} Regeln, ${used.size} Tokens, ${usedKeyframes.size ? [...usedKeyframes].join(' ') : '–'} → ${path.relative(bos, OUT)}`);
}
