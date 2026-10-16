// CHAT4: Geometrie der Chatleiste im v4-Rahmen gegen die Shell-V2. Bezug = dieselbe shared/business-chat.js im
// CSS-Kontext der Shell-V2 (ui-contract, app.css, base.css, mobile-host.css, desktop/index.css – wie FR6d-G0 im
// Rahmenlabor), mit einer In-Memory-Datenschicht und demselben gesäten Chat. Gleiche Bedienung (drive.mjs), dann
// Rechtecke von Leiste, Chatfenster, Verlauf, Eingabe, Senden und Eintrag je Breite (1280, 420) vergleichen (±2 px).
// Lauf (aus src/apps/business-os): node v4/shell/chat/tests/geometry.test.mjs
import { createRequire } from 'node:module';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { SEED, TYPED, openDock, typeInChat, measure } from './drive.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
process.env.CTOX_APP_API_STUB ||= path.join(os.homedir(), '.local/state/shell-v4/stub-pins/b6624eb');
const { startServer } = await import('../../../frame/lab/serve.mjs');
const require = createRequire(path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
const { chromium } = require('playwright');
const exe = path.join(os.homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');
const shots = path.join(here, 'shots');
fs.mkdirSync(shots, { recursive: true });

const results = [];
function check(name, ok, detail = '') {
  results.push({ name, ok: !!ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} [geometry] ${name}${detail ? ` – ${detail}` : ''}`);
}

const server = await startServer(0);
const base = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ executablePath: exe, headless: true });

async function v4Page(width) {
  const ctx = await browser.newContext({ viewport: { width, height: 800 } });
  const P = await ctx.newPage();
  await P.goto(`${base}/v4/frame/lab/stub-lab.html?inst=chat4g-${width}-${Date.now().toString(36)}`);
  await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  await P.evaluate(async (seed) => {
    const { kernel } = window.__lab;
    // Empfänger wie im V2-Bezug: Annahme mit task_id (die ctox-App nimmt business_os.chat.task an).
    kernel.sim.onCommand('business_os.chat.task', (payload) => ({ ok: true, task_id: 'queue-ref-1', chat_id: payload.chat_id, status: 'queued' }));
    const now = Date.now();
    await kernel.sim.push('business_chats', [{ id: seed.id, owner_user_id: kernel.appApi('shell', 'dev').actor.id, title: seed.title, createdAt: now - 60_000,
      updated_at_ms: now - 60_000, open: true, minimized: true, messages: [{ id: 'msg_seed_1', role: 'user', text: seed.text, createdAt: now - 60_000 }] }]);
    await kernel.sim.publishAppVersion('ctox', '1.0.0', { title: 'CTOX', layout: { category: 'Crew' }, styles: [], collections: { business_chats: {} } });
  }, SEED);
  await P.waitForSelector('[data-ctox-chat-root] [data-chat-dock]', { timeout: 8000 });
  return { P, ctx, theme: await P.evaluate(() => document.documentElement.dataset.theme) };
}

// CHATFIX: die dokumentierten v4-Korrekturen (v4/shell/chat/chat-v4.css, kein V2-Abbild) gelten auch in der V2-Referenz,
// ohne Hülle [data-v4-chat-v2]. Sonst würde die Referenz den V2-Fehler (Inspektionszeile weitet die Fensterspalte) als
// Sollwert festschreiben; alle übrigen Maße bleiben exakt gegen V2 geprüft.
const V4_FIXES_FOR_V2_REFERENCE = fs.readFileSync(path.join(here, '../chat-v4.css'), 'utf8').replaceAll('[data-v4-chat-v2] ', '');

async function v2Page(width, theme) {
  const ctx = await browser.newContext({ viewport: { width, height: 800 } });
  const P = await ctx.newPage();
  await P.goto(`${base}/v4/frame/lab/index.html?chat4-ref=1`).catch(() => {});
  await P.setContent(`<!doctype html><html lang="de" data-theme="${theme}" data-shell-style="ctox"><head><meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <link rel="stylesheet" href="${base}/ui-contract/v1/workjet-ui-contract.css"><link rel="stylesheet" href="${base}/app.css">
    <link rel="stylesheet" href="${base}/shared/base.css"><link rel="stylesheet" href="${base}/mobile-host.css">
    <link rel="stylesheet" href="${base}/modules/desktop/index.css"><style>${V4_FIXES_FOR_V2_REFERENCE}</style></head><body data-module-shell="full">
    <script type="module">
      import { initBusinessChat } from '${base}/shared/business-chat.js';
      const seed = ${JSON.stringify(SEED)};
      const now = Date.now();
      const chats = new Map([[seed.id, { id: seed.id, owner_user_id: 'owner', title: seed.title, createdAt: now - 60000, updated_at_ms: now - 60000,
        open: true, minimized: true, messages: [{ id: 'msg_seed_1', role: 'user', text: seed.text, createdAt: now - 60000 }] }]]);
      const wrap = (d) => ({ ...d, toJSON: () => structuredClone(d), incrementalPatch: async (p) => { chats.set(d.id, { ...d, ...p }); }, remove: async () => { chats.delete(d.id); } });
      const coll = (rows) => ({
        find: () => ({ exec: async () => rows().map(wrap), $: { subscribe: (cb) => { cb(rows().map(wrap)); return { unsubscribe() {} }; } } }),
        findOne: (id) => ({ exec: async () => { const r = rows().find((x) => x.id === id); return r ? wrap(r) : null; } }),
        insert: async (d) => { chats.set(d.id, d); },
        $: { subscribe: () => ({ unsubscribe() {} }) },
      });
      const none = () => [];
      const db = { raw: { business_chats: coll(() => [...chats.values()]), business_commands: coll(none), ctox_queue_tasks: coll(none),
        ctox_crew_members: coll(none), ctox_harness_status: coll(none), ctox_harness_events: coll(none) } };
      initBusinessChat({ session: { authenticated: true, user: { id: 'owner', name: 'Owner' } },
        commandBus: { dispatch: async (c) => ({ ok: true, command_id: c.id, task_id: 'queue-ref-1', status: 'queued' }) },
        db, sync: { startCollection: async () => ({ state: { awaitInSync: async () => true } }) }, getActiveModule: () => null });
      window.__refReady = true;
    </script></body></html>`, { waitUntil: 'load' });
  await P.waitForFunction(() => window.__refReady === true && document.querySelector('[data-ctox-chat-root] [data-chat-dock]'), null, { timeout: 8000 });
  return { P, ctx };
}

const near = (a, b, tol = 2) => Array.isArray(a) && Array.isArray(b) && a.every((v, i) => Math.abs(v - b[i]) <= tol);
try {
  for (const width of [1280, 420]) {
    const v4 = await v4Page(width);
    const v2 = await v2Page(width, v4.theme);
    const out = {};
    for (const [name, page] of [['v4', v4.P], ['v2', v2.P]]) {
      await page.mouse.move(width - 10, 10);
      await page.waitForTimeout(250);
      const collapsed = await measure(page);
      await openDock(page);
      await typeInChat(page, { send: false });
      await page.mouse.move(width - 10, 10);
      await page.waitForTimeout(400);
      out[name] = { collapsed, open: await measure(page) };
      await page.screenshot({ path: path.join(shots, `geometry-${width}-${name}.png`) });
      // Senden (Maus), dann der Zustand „Aufgabe steht auf der Aufgabenliste“.
      const send = await page.$(`[data-ctox-chat-root] [data-chat-id="${SEED.id}"] [data-chat-send]`);
      const sb = await send?.boundingBox();
      if (sb) await page.mouse.click(sb.x + sb.width / 2, sb.y + sb.height / 2);
      await page.waitForFunction((id) => /Aufgabenliste/.test(document.querySelector(`[data-ctox-chat-root] [data-chat-id="${id}"]`)?.textContent || ''), SEED.id, { timeout: 8000 }).catch(() => {});
      await page.mouse.move(width - 10, 10);
      await page.waitForTimeout(400);
      out[name].sent = await measure(page);
      await page.screenshot({ path: path.join(shots, `geometry-${width}-${name}-sent.png`) });
    }
    check(`${width}: Leiste eingeklappt gleich`, near(out.v4.collapsed.dock, out.v2.collapsed.dock), JSON.stringify({ v4: out.v4.collapsed.dock, v2: out.v2.collapsed.dock }));
    for (const key of ['dock', 'chip', 'window', 'messages', 'textarea', 'send']) {
      check(`${width}: ${key} offen gleich`, near(out.v4.open[key], out.v2.open[key]), JSON.stringify({ v4: out.v4.open[key], v2: out.v2.open[key] }));
    }
    for (const key of ['window', 'messages', 'textarea', 'send']) {
      check(`${width}: ${key} nach dem Senden gleich`, near(out.v4.sent[key], out.v2.sent[key]), JSON.stringify({ v4: out.v4.sent[key], v2: out.v2.sent[key] }));
    }
    check(`${width}: Verlauf nach dem Senden ohne waagerechten Überlauf wie V2`, out.v4.sent.overflowX === out.v2.sent.overflowX, JSON.stringify({ v4: out.v4.sent.overflowX, v2: out.v2.sent.overflowX }));
    check(`${width}: Schrift der Pille gleich`, out.v4.open.font === out.v2.open.font, JSON.stringify({ v4: out.v4.open.font, v2: out.v2.open.font }));
    check(`${width}: Fensterfarbe gleich`, out.v4.open.windowBg === out.v2.open.windowBg, JSON.stringify({ v4: out.v4.open.windowBg, v2: out.v2.open.windowBg }));
    check(`${width}: Verlauf ohne waagerechten Überlauf wie V2`, out.v4.open.overflowX === out.v2.open.overflowX, JSON.stringify({ v4: out.v4.open.overflowX, v2: out.v2.open.overflowX }));
    await v4.ctx.close();
    await v2.ctx.close();
  }
} finally {
  await browser.close();
  server.close();
}
const failed = results.filter((r) => !r.ok);
console.log(`\n[geometry] ${results.length - failed.length}/${results.length} PASS`);
fs.writeFileSync(path.join(shots, 'geometry-results.json'), JSON.stringify(results, null, 2));
process.exit(failed.length ? 1 : 0);
