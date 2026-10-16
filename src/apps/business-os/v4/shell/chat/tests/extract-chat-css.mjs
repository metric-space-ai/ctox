// Erzeugt v4/shell/chat/chat-v2.css (CHAT4; Werkzeug, liegt unter tests/ und gehört nicht ins Shell-Paket): der CSS-Kontext der Shell-V2, in dem shared/business-chat.js seine
// Chatleiste zeichnet – WÖRTLICH aus den Quellen in der Ladereihenfolge der Shell-V2 (index.html):
//   ui-contract/v1/workjet-ui-contract.css, app.css, mobile-host.css.
// Die eigenen Regeln der Leiste (installChatStyles) schreibt business-chat.js zur Laufzeit selbst in <head>, wie in V2;
// sie gehören nicht hierher. shared/base.css lädt der Rahmen ohnehin global.
// Lauf: node src/apps/business-os/v4/shell/chat/tests/extract-chat-css.mjs
// Verfahren wie v4/frame/lab/extract-desktop-css.mjs (Parser und Umschreibung aus extract-topbar-css.mjs), Hülle
// [data-v4-chat-v2] (v4/shell/chat/index.js hängt die Leiste hinein): Eine Regel kommt mit, wenn einer ihrer Selektoren
// ein Element der Probe trifft. Probe = die echte Leiste (initBusinessChat mit In-Memory-Daten) in vier Zuständen:
// eingeklappt, aufgeklappt mit offenem Chat, Tagesübersicht offen, Chat maximiert. Zustände (Hover/Fokus,
// Pseudo-Elemente, .is-*, Attribute) zählen für den Treffer nicht. Dazu Tokens (transitiv benutzte Variablen) und die
// Typografie von html/body an der Hülle.
import { createRequire } from 'node:module';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { parse, splitList, ROOT_RE, scopeSelector as scopeWith, testSelector } from '../../../frame/lab/extract-topbar-css.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const bos = path.resolve(here, '../../../..');
export const HULL = '[data-v4-chat-v2]';
const OUT = path.join(here, '..', 'chat-v2.css');
const SOURCES = ['ui-contract/v1/workjet-ui-contract.css', 'app.css', 'mobile-host.css'];
const scopeSelector = (s) => scopeWith(s, HULL);
const PROBE_HULL = '[data-v4-v2]'; // Treffer-Probe; die Ausgabe nutzt HULL

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const rules = SOURCES.flatMap((f) => parse(fs.readFileSync(path.join(bos, f), 'utf8'), f));
  const require = createRequire(path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
  const { chromium } = require('playwright');
  const exe = path.join(os.homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');
  const { startServer } = await import('../../../frame/lab/serve.mjs');
  const server = await startServer(0);
  const base = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium.launch({ executablePath: exe, headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  await page.goto(`${base}/v4/frame/lab/index.html?extract-chat=1`).catch(() => {});
  await page.setContent('<!doctype html><html lang="de"><head></head><body><div id="probe"></div></body></html>');
  await page.evaluate(async (b) => {
    const { initBusinessChat } = await import(`${b}/shared/business-chat.js`);
    const now = Date.now();
    const chats = new Map([['chat_probe', { id: 'chat_probe', owner_user_id: 'owner', title: 'Probe', createdAt: now - 60000, updated_at_ms: now - 60000,
      open: true, minimized: true, messages: [{ id: 'm1', role: 'user', text: 'Frage', createdAt: now - 60000 },
        { id: 'm2', role: 'ctox', text: 'Antwort', createdAt: now - 50000, detail: 'Kontext' }] }]]);
    const wrap = (d) => ({ ...d, toJSON: () => structuredClone(d), incrementalPatch: async (p) => { chats.set(d.id, { ...d, ...p }); }, remove: async () => {} });
    const coll = (rows) => ({
      find: () => ({ exec: async () => rows().map(wrap), $: { subscribe: (cb) => { cb(rows().map(wrap)); return { unsubscribe() {} }; } } }),
      findOne: (id) => ({ exec: async () => { const r = rows().find((x) => x.id === id); return r ? wrap(r) : null; } }),
      insert: async (d) => { chats.set(d.id, d); },
      $: { subscribe: () => ({ unsubscribe() {} }) },
    });
    const none = () => [];
    const members = () => [{ id: 'm_a', name: 'Anna', shape: 'round', color: '#c2410c', state: 'home', archived: false }];
    initBusinessChat({ session: { authenticated: true, user: { id: 'owner', name: 'Owner' } },
      commandBus: { dispatch: async (c) => ({ ok: true, command_id: c.id, task_id: 'q1', status: 'queued' }) },
      db: { raw: { business_chats: coll(() => [...chats.values()]), business_commands: coll(none), ctox_queue_tasks: coll(none),
        ctox_crew_members: coll(members), ctox_harness_status: coll(none), ctox_harness_events: coll(none) } },
      sync: { startCollection: async () => ({ state: { awaitInSync: async () => true } }) }, getActiveModule: () => null });
    const wait = (ms) => new Promise((r) => setTimeout(r, ms));
    const root = () => document.querySelector('[data-ctox-chat-root]');
    const probe = document.getElementById('probe');
    const snap = () => {
      const hull = document.createElement('div');
      hull.dataset.v4ChatV2 = '';
      hull.dataset.v4V2 = ''; // nur für die Treffer-Probe: testSelector (extract-topbar-css.mjs) behält [data-v4-v2]
      hull.dataset.shellStyle = 'ctox';
      hull.innerHTML = root().outerHTML.replace(/\sdata-ctox-chat-root="[^"]*"/, '');
      probe.appendChild(hull);
    };
    await wait(300);
    snap(); // eingeklappt
    root().querySelector('[data-chat-open]')?.click();
    await wait(300);
    if (!root().querySelector('[data-chat-id] textarea')) root().querySelector('[data-chat-focus]')?.click();
    await wait(300);
    snap(); // aufgeklappt, Chat offen
    root().querySelector('.ctox-date-picker-trigger')?.click();
    await wait(300);
    snap(); // Tagesübersicht
    root().querySelector('.ctox-date-picker-trigger')?.click();
    await wait(200);
    root().querySelector('[data-chat-maximize]')?.click();
    await wait(300);
    snap(); // maximiert
    root().__ctoxChatCleanup?.();
    root().remove();
  }, base);

  const uniq = [...new Set(rules.filter((r) => r.kind === 'rule').flatMap((r) => splitList(r.selector)))];
  const matches = new Map();
  for (const theme of ['light', 'dark']) {
    const res = await page.evaluate(([list, theme]) => {
      document.documentElement.dataset.theme = theme;
      const hulls = [...document.getElementById('probe').children];
      const out = {};
      for (const [scoped, test] of list) {
        if (!test) continue;
        try {
          if ([...document.querySelectorAll(test)].some((e) => hulls.some((h) => h === e || h.contains(e)))) out[scoped] = true;
        } catch { out[scoped] = 'invalid'; }
      }
      return out;
    }, [uniq.map((s) => [s, testSelector(scopeWith(s, PROBE_HULL))]), theme]);
    for (const [k, v] of Object.entries(res)) if (v === true) matches.set(k, true);
  }
  await browser.close();
  server.close();

  // ---------- Ausgabe (wie extract-desktop-css.mjs) ----------
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
  // Die Laufzeitregeln der Leiste (installChatStyles) lesen dieselben Tokens; sie gehören zur benutzten Menge.
  const chatSrc = fs.readFileSync(path.join(bos, 'shared/business-chat.js'), 'utf8');
  varRefs(chatSrc).forEach((v) => used.add(v));
  const rootRules = rules.filter((r) => r.kind === 'rule' && splitList(r.selector).every(isRootOnly) && splitList(r.selector).some((x) => tokenSel(x)));
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

  let css = `/* GENERIERT von v4/shell/chat/tests/extract-chat-css.mjs – nicht von Hand ändern (CHAT4, 16.10.2026).
   CSS-Kontext der Shell-V2-Chatleiste wörtlich aus ${SOURCES.join(', ')}: Deklarationen, @media und Reihenfolge
   unverändert, Selektoren mechanisch auf die Hülle ${HULL} begrenzt (Spezifität gleich). Die Regeln der Leiste selbst
   schreibt shared/business-chat.js (installChatStyles) zur Laufzeit. Änderungen nur in den Quellen, dann neu erzeugen;
   v4/shell/chat/tests/geometry.test.mjs vergleicht die Leiste mit der Shell-V2. */\n\n`;
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
