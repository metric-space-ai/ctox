// CHAT4: Abnahme der V2-Chatleiste im v4-Rahmen gegen den Kern-Stub (Pin 8265bd7), Playwright mit echter Maus/Tastatur.
// Lauf (aus src/apps/business-os): node v4/shell/chat/tests/chat.test.mjs [counter]
//   counter = Gegenprobe: v4/shell/chat/index.js wird durch ein Modul ersetzt, das nichts montiert; (a)–(c) müssen rot sein.
// Fälle: (a) Leiste unten links bei 1280 und 420, genau eine Leiste (Crew-Leiste abgetreten); (b) Chat öffnen, Text
// tippen, senden → genau ein Befehl business_os.chat.task mit erwarteter Nutzlast; (c) ein in business_chats gesäter
// Chat wird angezeigt.
import { createRequire } from 'node:module';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { SEED, TYPED, openDock, typeInChat } from './drive.mjs';
import { realManifestRoutes } from '../../../tools/port/real-manifest.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
process.env.CTOX_APP_API_STUB ||= path.join(os.homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
const { startServer } = await import('../../../frame/lab/serve.mjs');
const require = createRequire(path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
const { chromium } = require('playwright');
const exe = path.join(os.homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');
const COUNTER = process.argv[2] === 'counter';
const MODE = COUNTER ? 'counter' : 'chat';
const shots = path.join(here, 'shots');
fs.mkdirSync(shots, { recursive: true });
const shot = (name) => path.join(shots, `${MODE}-${name}.png`);

const results = [];
function check(name, ok, detail = '') {
  results.push({ name, ok: !!ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} [${MODE}] ${name}${detail ? ` – ${detail}` : ''}`);
}


const server = await startServer(0);
const base = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ executablePath: exe, headless: true });
const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
const P = await ctx.newPage();
const perr = [];
P.on('pageerror', (e) => perr.push(String(e)));
P.on('console', (m) => { if (m.type() === 'error' && !/404|Failed to load resource/.test(m.text())) perr.push(m.text()); });
if (COUNTER) {
  await P.route(/\/v4\/shell\/chat\/index\.js(\?.*)?$/, (route) => route.fulfill({ status: 200, contentType: 'text/javascript',
    body: 'export const CHAT_COLLECTIONS = []; export function mountBusinessChat() { return null; } export function currentBusinessChat() { return null; }' }));
}

try {
  await P.goto(`${base}/v4/frame/lab/stub-lab.html?inst=chat4-${Date.now().toString(36)}`);
  await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  // Empfänger des Chatbefehls (wie die ctox-App, deren Handler business_os.chat.task annimmt) und Saat in business_chats.
  await P.evaluate(async ([seed, ctoxRoutes]) => {
    const { kernel } = window.__lab;
    window.__chat4Cmds = [];
    kernel.sim.onCommand('business_os.chat.task', (payload, c) => {
      window.__chat4Cmds.push({ payload, app_id: c.app_id, command_id: c.command_id });
      return { ok: true, task_id: `queue-chat4-${window.__chat4Cmds.length}`, chat_id: payload.chat_id, status: 'queued' };
    });
    const now = Date.now();
    await kernel.sim.push('business_chats', [{
      id: seed.id, owner_user_id: kernel.appApi('shell', 'dev').actor.id, title: seed.title, createdAt: now - 60_000, updated_at_ms: now - 60_000,
      open: true, minimized: true, messages: [{ id: 'msg_seed_1', role: 'user', text: seed.text, createdAt: now - 60_000 }],
    }]);
    // Crew-App wie im Rahmenlabor (taskbar_group 'crew'): ohne Übergabe stünde ihre Crew-Leiste ebenfalls unten links.
    const svg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" fill="#0f766e"/></svg>';
    await kernel.sim.publishAppVersion('crew-ben', '1.0.0', { title: 'Crew Ben', layout: { category: 'Crew', icon_svg: svg, presentation: { taskbar_group: 'crew' } }, styles: [] });
    // E48: Eigentümer der Shell-Fremdlesungen (business_chats, ctox_crew_members, …) mit echtem Manifest (collections +
    // exports), sonst liest die Shell sie nicht.
    await kernel.sim.publishAppVersion('ctox', '1.0.0', { title: 'CTOX', layout: { category: 'Crew', icon_svg: svg }, styles: [], manifestExtra: ctoxRoutes });
  }, [SEED, realManifestRoutes('ctox')]);
  await P.waitForSelector('[data-v4-app="ctox"]', { state: 'attached' });
  await P.waitForSelector('[data-ctox-chat-root] [data-chat-dock]', { timeout: 8000 }).catch(() => {});

  // ---------- (a) Leiste unten links, genau eine ----------
  const docks = async () => P.evaluate(() => {
    const vis = (el) => { if (!el) return null; const r = el.getBoundingClientRect(); const cs = getComputedStyle(el); return r.width > 0 && r.height > 0 && cs.visibility !== 'hidden' && !el.closest('[hidden]') ? r : null; };
    const chat = vis(document.querySelector('[data-ctox-chat-root] [data-chat-dock]'));
    const crew = vis(document.querySelector('[data-v4-crew-bar] .ctox-chat-dock'));
    const all = [chat, crew].filter(Boolean);
    return { chat: chat && { left: chat.left, bottom: chat.bottom, width: chat.width, height: chat.height }, crew: !!crew, count: all.length, vw: innerWidth, vh: innerHeight };
  });
  const a1280 = await docks();
  check('(a) 1280: Chatleiste unten links', a1280.chat && a1280.chat.left <= 24 && a1280.vh - a1280.chat.bottom <= 32 && a1280.vh - a1280.chat.bottom >= 0, JSON.stringify(a1280));
  check('(a) 1280: genau eine Leiste unten links (Crew-Leiste abgetreten)', a1280.count === 1 && !a1280.crew, JSON.stringify(a1280));
  await P.screenshot({ path: shot('1280-collapsed') });

  // ---------- (c) gesäter Chat erscheint ----------
  // Leiste mit der Maus aufklappen (Pille), dann muss der gesäte Chat als Eintrag in der Leiste stehen.
  const seen = await openDock(P);
  check('(c) gesäter Chat aus business_chats steht in der Leiste', seen.chip && (seen.title + seen.text).includes(SEED.title), JSON.stringify(seen));

  // ---------- (b) Chat öffnen, tippen, senden → genau ein Befehl ----------
  await P.screenshot({ path: shot('1280-dock-open') });
  if (await typeInChat(P, { send: false })) {
    await P.screenshot({ path: shot('1280-open-typed') });
    const send = await P.$(`[data-ctox-chat-root] [data-chat-id="${SEED.id}"] [data-chat-send]`);
    const s = await send?.boundingBox();
    if (s) await P.mouse.click(s.x + s.width / 2, s.y + s.height / 2);
  }
  await P.waitForFunction(() => (window.__chat4Cmds || []).length > 0, null, { timeout: 8000 }).catch(() => {});
  await P.waitForTimeout(800); // Doppelsendung abwarten
  const sent = await P.evaluate(() => {
    const api = window.__lab.kernel.appApi('shell', 'dev');
    const docs = api.query('business_commands', {}).get().filter((d) => d.type === 'business_os.chat.task');
    return { handled: window.__chat4Cmds, docs: docs.map((d) => ({ id: d.id, type: d.type, app_id: d.app_id, record_id: d.record_id ?? null, status: d.status })) };
  });
  const cmd = sent.handled[0]?.payload || {};
  check('(b) genau ein Befehl business_os.chat.task geschrieben und behandelt', sent.handled.length === 1 && sent.docs.length === 1, JSON.stringify({ handled: sent.handled.length, docs: sent.docs }));
  check('(b) Nutzlast: prompt/instruction = getippter Text, chat_id = Chat, Kanal business_os.llm.chat → business_os_chat',
    cmd.prompt === TYPED && cmd.instruction === TYPED && cmd.chat_id === SEED.id && cmd.reply_to === SEED.id
      && cmd.inbound_channel === 'business_os.llm.chat' && cmd.outbound_channel === 'business_os_chat' && cmd.thread_key === `business-os/chat/${SEED.id}`,
    JSON.stringify({ prompt: cmd.prompt, chat_id: cmd.chat_id, inbound: cmd.inbound_channel, outbound: cmd.outbound_channel, thread: cmd.thread_key }));
  check('(b) Absender App shell, Datensatzbezug = Chat', sent.handled[0]?.app_id === 'shell' && sent.docs[0]?.record_id === SEED.id, JSON.stringify(sent.docs[0] || null));
  await P.waitForFunction((seed) => {
    const node = document.querySelector(`[data-ctox-chat-root] [data-chat-id="${seed.id}"]`);
    return node && /Aufgabenliste/.test(node.textContent || '');
  }, SEED, { timeout: 5000 }).catch(() => {});
  const echoed = await P.evaluate(([seed, typed]) => {
    const node = document.querySelector(`[data-ctox-chat-root] [data-chat-id="${seed.id}"]`);
    const t = node?.textContent || '';
    return { typed: t.includes(typed), accepted: /Aufgabenliste/.test(t) };
  }, [SEED, TYPED]);
  check('(b) Verlauf zeigt die Nachricht und die Annahme der Crew', echoed.typed && echoed.accepted, JSON.stringify(echoed));
  // Persistenz: der Chat-Verlauf landet über api.put in business_chats (lokal), mit der neuen Nachricht.
  await P.waitForTimeout(400);
  const stored = await P.evaluate(([seed, typed]) => {
    const row = window.__lab.kernel.appApi('shell', 'dev').get('business_chats', seed.id).get();
    return { found: !!row, typed: !!row?.messages?.some((m) => m.text === typed), owner: row?.owner_user_id ?? null };
  }, [SEED, TYPED]);
  check('(b) Verlauf in business_chats gespeichert (api.put)', stored.found && stored.typed, JSON.stringify(stored));
  // CHATFIX: ein langer Status nach dem Senden weitet die Fensterspalte nicht (V2-Fehler, in v4 behoben).
  const col = await P.evaluate(() => { const w = document.querySelector('.ctox-chat-window'); return { win: Math.round(w.getBoundingClientRect().width), col: Math.round(parseFloat(getComputedStyle(w).gridTemplateColumns)), status: w.querySelector('.ctox-chat-inspection > summary')?.textContent?.trim()?.length ?? 0 }; });
  check('(b) langer Status weitet die Fensterspalte nicht (Nachrichten nicht abgeschnitten)', col.status > 40 && col.col <= col.win, JSON.stringify(col));
  await P.screenshot({ path: shot('1280-open-sent') });

  // ---------- (a) 420 ----------
  await P.setViewportSize({ width: 420, height: 800 });
  await P.waitForTimeout(400);
  const a420 = await docks();
  check('(a) 420: Chatleiste unten links', a420.chat && a420.chat.left <= 24 && a420.vh - a420.chat.bottom <= 32 && a420.vh - a420.chat.bottom >= 0 && a420.chat.left + a420.chat.width <= 420, JSON.stringify(a420));
  check('(a) 420: genau eine Leiste unten links', a420.count === 1 && !a420.crew, JSON.stringify(a420));
  const overflow = await P.evaluate(() => document.documentElement.scrollWidth <= innerWidth);
  check('(a) 420: kein waagerechtes Überlaufen der Seite', overflow);
  await P.screenshot({ path: shot('420-open') });

  // ---------- Shell-Tausch (Doppelpuffer): der neue Rahmen übernimmt die Leiste, genau eine bleibt ----------
  await P.setViewportSize({ width: 1280, height: 800 });
  const swap = await P.evaluate(async (seed) => {
    const r = await window.__lab.swapShell();
    await new Promise((res) => setTimeout(res, 600));
    const roots = document.querySelectorAll('[data-ctox-chat-root]').length;
    const hulls = document.querySelectorAll('[data-v4-chat-v2]').length;
    const crewBars = [...document.querySelectorAll('.v4f-crew')].filter((e) => !e.hidden && e.isConnected).length;
    const chip = !!document.querySelector(`[data-ctox-chat-root] [data-chat-focus="${seed.id}"]`);
    return { swapped: r.swapped, error: r.error ? String(r.error) : null, roots, hulls, crewBars, chip };
  }, SEED);
  check('Shell-Tausch: genau eine Chatleiste, Crew-Leiste weiter abgetreten, Verlauf da', swap.swapped !== false && !swap.error && swap.roots === 1 && swap.hulls === 1 && swap.crewBars === 0 && swap.chip, JSON.stringify(swap));

  // ---------- Rückgabe: ohne ctox-App tritt die Leiste ab, die Crew-Leiste kommt zurück ----------
  await P.evaluate(() => window.__lab.remove('ctox'));
  await P.waitForFunction(() => !document.querySelector('[data-ctox-chat-root]'), null, { timeout: 5000 }).catch(() => {});
  const back = await docks();
  check('ohne ctox-App: keine Chatleiste, Crew-Leiste wieder da', !back.chat && back.crew && back.count === 1, JSON.stringify(back));
  check('keine Seitenfehler', perr.length === 0, perr.slice(0, 3).join(' | '));
} finally {
  await browser.close();
  server.close();
}

const failed = results.filter((r) => !r.ok);
console.log(`\n[${MODE}] ${results.length - failed.length}/${results.length} PASS`);
fs.writeFileSync(path.join(shots, `${MODE}-results.json`), JSON.stringify(results, null, 2));
process.exit(COUNTER ? 0 : (failed.length ? 1 : 0));
