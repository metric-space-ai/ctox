// Strang MAENGEL: Abnahme der Owner-Mängel M-01 … M-20 (owner-maengel.md) im v4-Prototyp, so wie der Owner sie erlebt hat.
// Nur messen, nichts ändern. Echte Maus (page.mouse.click auf die Elementmitte, vorher elementFromPoint), 1280×800,
// wo es um Breite geht zusätzlich 420×800. Screenshots: shots/maengel-M-XX-*.png.
//
// Lauf (aus src/apps/business-os):
//   CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/64acc2c node v4/frame/lab/maengel.mjs [M01,M02,…]
// Rahmen-Fälle laufen gegen lab/stub-lab.html (Kern-Stub, gebundene Shell-URL wie im echten Ladeweg), echte Apps
// (credentials, coding-agents, reports, news) über den Server des Geometrie-Labors (v4/tools/geometry/geometry-v4.mjs).
import { createRequire } from 'node:module';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { realManifestRoutes } from '../../tools/port/real-manifest.mjs';
// Default-Pin vor dem Laden von serve.mjs/geometry-v4.mjs setzen (serve.mjs teilt seinen Default mit anderen Laboren).
process.env.CTOX_APP_API_STUB ||= path.join(os.homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
const { startServer } = await import('./serve.mjs');
const { serve: geoServe, appConfig, configs } = await import('../../tools/geometry/geometry-v4.mjs');

const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
const { chromium } = require('playwright');
const exe = path.join(os.homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');
const SHOTS = path.join(here, 'shots');
const ONLY = (process.argv[2] || '').split(',').filter(Boolean);
const want = (k) => !ONLY.length || ONLY.includes(k);
const out = [];
function rec(m, name, ok, detail = {}) {
  out.push({ m, name, ok: !!ok, detail });
  console.log(`${ok ? 'JA  ' : 'NEIN'} ${m} ${name} ${JSON.stringify(detail).slice(0, 900)}`);
}
const shotPath = (m, name) => path.join(SHOTS, `maengel-${m}-${name}.png`);

const server = await startServer(0);
const base = `http://127.0.0.1:${server.address().port}`;
const geo = await geoServe();
const geoBase = `http://127.0.0.1:${geo.address().port}`;
const browser = await chromium.launch({ executablePath: exe, headless: true });

// ---------- echte Maus ----------
// Mitte des Elements, Treffertest (elementFromPoint = Element oder Nachfahre), dann mouse.move + mouse.click.
async function mclick(P, sel, { button = 'left', must = true } = {}) {
  const h = typeof sel === 'string' ? await P.$(sel) : sel;
  if (!h) { if (must) throw new Error(`mclick: ${sel} fehlt`); return { ok: false, why: 'fehlt' }; }
  await h.scrollIntoViewIfNeeded().catch(() => {});
  const r = await h.evaluate((el) => { const b = el.getBoundingClientRect(); const x = b.left + b.width / 2; const y = b.top + b.height / 2; const hit = document.elementFromPoint(x, y); return { x, y, w: b.width, h: b.height, hit: !!hit && (hit === el || el.contains(hit)), hitTag: hit ? `${hit.tagName.toLowerCase()}.${[...hit.classList].slice(0, 2).join('.')}` : null }; });
  if (!r.hit) { if (must) throw new Error(`mclick: ${sel} verdeckt von ${r.hitTag}`); return { ok: false, ...r }; }
  await P.mouse.move(r.x, r.y, { steps: 4 });
  await P.mouse.click(r.x, r.y, { button });
  return { ok: true, ...r };
}
async function mhover(P, sel) {
  const h = await P.$(sel);
  const r = await h.evaluate((el) => { const b = el.getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; });
  await P.mouse.move(r.x - 40, r.y + 40); await P.mouse.move(r.x, r.y, { steps: 6 });
  return r;
}
const rect = (P, sel) => P.$eval(sel, (e) => { const r = e.getBoundingClientRect(); return { l: Math.round(r.left), t: Math.round(r.top), r: Math.round(r.right), b: Math.round(r.bottom), w: Math.round(r.width), h: Math.round(r.height) }; });

// ---------- Rahmen-Labor (Kern-Stub, gebundene Shell) ----------
const CREW = [['crew-anna', 'Anna', '#c2410c'], ['crew-ben', 'Ben', '#0f766e'], ['crew-cleo', 'Cleo', '#7c3aed']];
const crmSvg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" fill="#7b5cc4"/><text x="32" y="42" font-family="system-ui,sans-serif" font-size="28" font-weight="600" fill="#fff" text-anchor="middle">K</text></svg>';
const MOD = path.resolve(here, '../../../modules');
const notesDir = path.join(here, 'apps/notes/1.0.0');
async function lab({ w = 1280, h = 800, crew = false, extra = true, source = false } = {}) {
  const ctx = await browser.newContext({ viewport: { width: w, height: h }, locale: 'de-DE' });
  const P = await ctx.newPage();
  const perr = [];
  P.on('pageerror', (e) => perr.push(String(e)));
  P.on('console', (m) => { if (m.type() === 'error' && !/404|Failed to load resource|ctox:lib\/monaco/.test(m.text())) perr.push(m.text()); });
  await P.route(/\/~a\/crew-[a-z]+@/, (route) => {
    const id = /\/~a\/(crew-[a-z]+)@/.exec(route.request().url())[1];
    const name = CREW.find((c) => c[0] === id)?.[1] || id;
    route.fulfill({ status: 200, contentType: 'text/javascript', body: `export function mount(root, api) {
      root.innerHTML = '<div style="font:13px system-ui;padding:12px 14px"><p>Crew-Chat</p><input placeholder="Nachricht" style="width:90%;height:28px"></div>';
      api.shell?.setTitle?.('Chat ${name}');
      return { unmount() { root.replaceChildren(); }, snapshot() { return {}; } };
    }` });
  });
  await P.route(/\/~a\/notes@(?!1\.0\.0)[^/]+\//, (route) => {
    const rest = new URL(route.request().url()).pathname.replace(/^.*\/~a\/notes@[^/]+\//, '');
    const f = path.join(notesDir, rest);
    if (!f.startsWith(notesDir) || !fs.existsSync(f)) { route.fulfill({ status: 404 }); return; }
    route.fulfill({ status: 200, contentType: rest.endsWith('.svg') ? 'image/svg+xml' : 'text/javascript', body: fs.readFileSync(f) });
  });
  // echte App „Coding Agents“ (wie frame.test.mjs ag())
  const caDir = path.join(MOD, 'coding-agents');
  await P.route(/\/~a\/coding-agents@[^/]+\//, (route) => {
    const rest = new URL(route.request().url()).pathname.replace(/^.*\/~a\/coding-agents@[^/]+\//, '');
    const f = path.join(caDir, rest);
    if (!f.startsWith(caDir) || !fs.existsSync(f)) { route.fulfill({ status: 404 }); return; }
    const type = rest.endsWith('.css') ? 'text/css' : rest.endsWith('.html') ? 'text/html' : rest.endsWith('.json') ? 'application/json' : rest.endsWith('.svg') ? 'image/svg+xml' : 'text/javascript';
    route.fulfill({ status: 200, contentType: type, body: fs.readFileSync(f) });
  });
  await P.goto(`${base}/v4/frame/lab/stub-lab.html?shell=bound&inst=mg-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`);
  await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  const caIcon = JSON.parse(fs.readFileSync(path.join(caDir, 'module.json'), 'utf8')).layout.icon_svg;
  // FIXT-R (Pin 4b0d0ac): coding-agents mit Routen/collections des echten Manifests, Eigentümer (ctox-platform) zuerst –
  // sonst sperrt der Kern das Lesen von coding_turns und die Shell schließt das Fenster gleich wieder.
  const caRoutes = realManifestRoutes('coding-agents');
  // …und die Eigentümer der Shell-Fremdlesungen (ctox: business_module_reports, business_users) vor dem ersten Öffnen der
  // Code-Ansicht – eine schon offene Meldungsliste sieht einen später veröffentlichten Eigentümer nicht.
  const shellOwners = (realManifestRoutes('shell')._fixture_owners ?? []).filter((o) => o.app === 'ctox');
  await P.evaluate(async ([extra, crew, crewList, crmSvg, caIcon, caRoutes, shellOwners]) => {
    const sim = window.__lab.kernel.sim;
    const add = (id, title, layout, ver = '1.0.0', styles = [], manifestExtra) => sim.publishAppVersion(id, ver, { title, layout, styles, ...(manifestExtra ? { manifestExtra } : {}) });
    if (extra) {
      await add('studio', 'Black Hole Studio', { category: 'Arbeit', frame_palette: { start: '#00ff00', middle: '#00ff00', end: '#00ff00', top_joint: '#00ff00', left_joint: '#00ff00' }, default_width: 900, default_height: 520 });
      await add('crm', 'Kontakte', { category: 'Vertrieb', icon_svg: crmSvg, default_width: 900, default_height: 520 }, '2.0.0');
      for (const o of [...(caRoutes._fixture_owners ?? []), ...shellOwners]) if (!(await sim.inspect('business_apps', o.app))?.data?.active_ver) await sim.publishAppVersion(o.app, o.ver, o.publish);
      const { _fixture_owners, ...caExtra } = caRoutes;
      await add('coding-agents', 'Coding Agents', { icon_svg: caIcon, category: 'Entwicklung', default_width: 560, default_height: 440 }, '1.0.0', ['index.css'], caExtra); // v4-allow: en (Laborprüfung: App-Titel)
    }
    if (crew) for (const [id, name, color] of crewList) {
      const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" fill="${color}"/><circle cx="32" cy="26" r="11" fill="#fff"/><path d="M12 58c3-13 12-19 20-19s17 6 20 19" fill="#fff"/></svg>`;
      await add(id, `Crew ${name}`, { category: 'Crew', icon_svg: svg, default_width: 420, default_height: 320, presentation: { taskbar_group: 'crew' } });
    }
  }, [extra, crew, CREW, crmSvg, caIcon, caRoutes, shellOwners]);
  if (extra) await P.waitForSelector('[data-v4-app="studio"]', { state: 'attached' });
  if (source) {
    const seeded = await P.evaluate(async (text) => { const src = window.__lab.kernel.source('notes'); await src.draft.write('index.js', text); return src.draft.publish({}).result; }, fs.readFileSync(path.join(notesDir, 'index.js'), 'utf8'));
    if (seeded?.status !== 'accepted') throw new Error('Quelle notes nicht veröffentlicht ' + JSON.stringify(seeded));
  }
  const open = (id, g) => P.evaluate(async ([id, g]) => { const hh = window.__lab.frame.shellHandle(); const o = hh.openWindow(id, g ? { geometry: g } : {}); await hh.win(o).host.ready; return o; }, [id, g]);
  const sel = async (o) => `#${await P.evaluate((o) => window.__lab.frame.shellHandle().win(o).handle.element.id, o)}`;
  return { ctx, P, perr, open, sel };
}
const W = (id) => `.shell-window[data-window-app="${id}"]`;
const settle = (P, ms = 400) => P.waitForTimeout(ms);

try {
  // ================= M-01 Layout-Menü räumlich =================
  if (want('M01')) {
    const { ctx, P, open, perr } = await lab();
    const o = await open('notes', { x: 360, y: 160, w: 560, h: 380 });
    await settle(P, 500);
    await mhover(P, `${W('notes')} [data-window-layout-trigger]`);
    await P.waitForSelector(`${W('notes')} [data-window-layout-menu]:not([hidden])`, { timeout: 3000 }).catch(() => {});
    await settle(P, 300);
    const menu = await P.$eval(`${W('notes')} [data-window-layout-menu]`, (m) => {
      const mr = m.getBoundingClientRect();
      return { open: !m.hidden, box: [Math.round(mr.left), Math.round(mr.top), Math.round(mr.width), Math.round(mr.height)],
        items: [...m.querySelectorAll('[data-window-layout-control]')].map((b) => { const r = b.getBoundingClientRect(); return { id: b.dataset.windowLayoutControl, label: b.getAttribute('aria-label'), tip: b.title || null, cx: Math.round(r.left + r.width / 2 - mr.left), cy: Math.round(r.top + r.height / 2 - mr.top) }; }) };
    });
    await P.screenshot({ path: shotPath('M-01', 'menue-1280') });
    const it = Object.fromEntries(menu.items.map((i) => [i.id, i]));
    const spatial = it.left && it.right && it['bottom-right'] && it['bottom-left'] && it.left.cx < it.maximize.cx && it.right.cx > it.maximize.cx && it['bottom-right'].cx > it['bottom-left'].cx && it['bottom-right'].cy > it['top-right'].cy && it['bottom-left'].cy > it['top-left'].cy;
    rec('M-01', 'Menü offen nach Hover, Symbole räumlich (links links, rechts rechts, unten rechts unten rechts)', menu.open && spatial, menu);
    const c1 = await mclick(P, `${W('notes')} [data-window-layout-control="left"]`);
    await settle(P, 700);
    const afterLeft = await rect(P, W('notes'));
    const desk = await P.evaluate(() => { const d = document.querySelector('[data-v4-desk], .v4f-desk, .desktop-surface') || document.body; const r = d.getBoundingClientRect(); return { l: r.left, r: r.right, w: r.width, t: r.top, b: r.bottom }; });
    await P.screenshot({ path: shotPath('M-01', 'nach-links-1280') });
    rec('M-01', 'echter Klick auf „Linke Hälfte“ → Fenster in der linken Hälfte', c1.ok && afterLeft.l <= desk.l + 12 && afterLeft.r <= desk.l + desk.w / 2 + 12 && afterLeft.w > desk.w / 2 - 40, { c1: c1.ok, afterLeft, desk });
    await mhover(P, `${W('notes')} [data-window-layout-trigger]`);
    await P.waitForSelector(`${W('notes')} [data-window-layout-menu]:not([hidden])`, { timeout: 3000 }).catch(() => {});
    await settle(P, 250);
    const c2 = await mclick(P, `${W('notes')} [data-window-layout-control="bottom-right"]`);
    await settle(P, 700);
    const afterBR = await rect(P, W('notes'));
    await P.screenshot({ path: shotPath('M-01', 'nach-unten-rechts-1280') });
    rec('M-01', 'echter Klick auf „Unten rechts anordnen“ → Fenster unten rechts', c2.ok && afterBR.l >= desk.l + desk.w / 2 - 12 && afterBR.t >= desk.t + (desk.b - desk.t) / 2 - 30, { afterBR });
    rec('M-01', 'ohne Seitenfehler', perr.length === 0, perr.slice(0, 3));
    await ctx.close();
  }

  // ================= M-02 Code-Ansicht + Rückweg; M-03/M-05 Agent im Code; M-06 Kreislauf =================
  if (want('M02') || want('M03') || want('M05') || want('M06')) {
    const { ctx, P, open, perr } = await lab({ source: true });
    const winsBefore = await P.$$eval('.shell-window', (e) => e.length);
    const o = await open('notes', { x: 40, y: 30, w: 1100, h: 640 });
    await settle(P, 500);
    // App-Menü am Icon-Header (echte Maus auf das Icon)
    await mclick(P, `${W('notes')} .shell-window-v2-icon`);
    await settle(P, 250);
    const menu1 = await P.$$eval(`${W('notes')} [data-v4-appmenu]`, (els) => els.map((e) => [e.dataset.v4Appmenu, e.textContent.trim()]));
    await P.screenshot({ path: shotPath('M-02', 'appmenue-1280') });
    rec('M-02', 'Klick auf Icon-Header öffnet App-Menü mit „Code ansehen“', menu1.some(([k]) => k === 'code'), menu1);
    await mclick(P, `${W('notes')} [data-v4-appmenu="code"]`);
    await P.waitForSelector(`${W('notes')}[data-v4-mode="code"]`, { timeout: 8000 }).catch(() => {});
    await settle(P, 1500);
    const cv = await P.$eval(W('notes'), (w) => {
      const vis = (e) => !!e && e.checkVisibility() && e.getBoundingClientRect().width > 0;
      const btns = [...w.querySelectorAll('[data-v4-mode-pane="code"] button, .v4-src button')].filter(vis).map((b) => (b.getAttribute('aria-label') || b.textContent).trim()).filter(Boolean);
      return { mode: w.dataset.v4Mode, sameWin: true, tree: vis(w.querySelector('.v4-src__tree, [data-src-tree], .v4-src__files')), monaco: vis(w.querySelector('.monaco-editor')), textarea: vis(w.querySelector('.v4-src textarea')),
        agent: vis(w.querySelector('[data-v4-agent]')), tabs: [...w.querySelectorAll('[data-src-tab]')].map((t) => t.textContent.trim()), buttons: [...new Set(btns)].slice(0, 30) };
    });
    const winsAfter = await P.$$eval('.shell-window', (e) => e.length);
    await P.screenshot({ path: shotPath('M-02', 'code-ansicht-1280') });
    rec('M-02', 'Code-Ansicht im selben Fenster: Dateibaum, Editor, Agent-Leiste, wenige Knöpfe', cv.mode === 'code' && winsAfter === winsBefore + 1 && cv.tree && (cv.monaco || cv.textarea), { ...cv, winsBefore, winsAfter });
    // Rückweg 1: Icon-Header → „Zurück zur App“
    await mclick(P, `${W('notes')} .shell-window-v2-icon`);
    await settle(P, 250);
    const menu2 = await P.$$eval(`${W('notes')} [data-v4-appmenu]`, (els) => els.map((e) => [e.dataset.v4Appmenu, e.textContent.trim()]));
    await P.screenshot({ path: shotPath('M-02', 'appmenue-im-code-1280') });
    const back = await mclick(P, `${W('notes')} [data-v4-appmenu="app"]`, { must: false });
    await settle(P, 500);
    const mode1 = await P.$eval(W('notes'), (w) => w.dataset.v4Mode);
    rec('M-02', 'Rückweg über das App-Menü „Zurück zur App“ (echter Klick)', back.ok && mode1 === 'app', { menu2, mode1 });
    // Rückweg 2: Esc (Fokus im Editor)
    await P.keyboard.press('Control+Shift+KeyE');
    await settle(P, 800);
    const mode2 = await P.$eval(W('notes'), (w) => w.dataset.v4Mode);
    const edH = await P.evaluateHandle((sel) => { const c = [...document.querySelectorAll(`${sel} .monaco-editor, ${sel} .v4-src textarea`)].filter((e) => e.checkVisibility() && e.getBoundingClientRect().width > 50); c.sort((a, b) => b.getBoundingClientRect().width * b.getBoundingClientRect().height - a.getBoundingClientRect().width * a.getBoundingClientRect().height); return c[0] || null; }, W('notes'));
    const ed = edH.asElement();
    const edClick = ed ? await mclick(P, ed, { must: false }) : null;
    await settle(P, 300);
    const focusEd = await P.evaluate(() => `${document.activeElement?.tagName}.${document.activeElement?.className?.toString().slice(0, 40)}|inEditor=${!!document.activeElement?.closest?.('.monaco-editor')}`);
    await P.keyboard.press('Escape');
    await settle(P, 500);
    const mode3 = await P.$eval(W('notes'), (w) => w.dataset.v4Mode);
    await P.keyboard.press('Escape');
    await settle(P, 500);
    const mode3b = await P.$eval(W('notes'), (w) => w.dataset.v4Mode);
    const focusInfo = await P.evaluate(() => `${document.activeElement?.tagName}.${document.activeElement?.className?.toString().slice(0, 40)}`);
    rec('M-02', 'Strg+Umschalt+E öffnet Code, Esc (nach Klick in den Editor, Monaco) führt zurück zur App', mode2 === 'code' && mode3 === 'app', { mode2, mode3, mode3b_zweitesEsc: mode3b, edClick: edClick && { ok: edClick.ok, hit: edClick.hitTag }, focusEd, focusInfo });
    if (mode3b === 'code') {
      // Esc mit Fokus außerhalb des Editors (Klick auf den Reiter „Dateien“)
      await mclick(P, `${W('notes')} [data-src-tab="files"]`, { must: false });
      await P.keyboard.press('Escape');
      await settle(P, 500);
      const mode4 = await P.$eval(W('notes'), (w) => w.dataset.v4Mode);
      rec('M-02', 'Esc mit Fokus außerhalb des Editors (Reiter „Dateien“) führt zurück zur App', mode4 === 'app', { mode4 });
      if (mode4 === 'code') { await mclick(P, `${W('notes')} .shell-window-v2-icon`); await settle(P, 200); await mclick(P, `${W('notes')} [data-v4-appmenu="app"]`); await settle(P, 400); }
    }
    await P.screenshot({ path: shotPath('M-02', 'zurueck-in-app-1280') });

    // ---- M-03/M-05: Agent in der Code-Ansicht, Auftrag per Tastatur, Entwurf ändert sich ----
    await mclick(P, `${W('notes')} .shell-window-v2-icon`);
    await settle(P, 200);
    await mclick(P, `${W('notes')} [data-v4-appmenu="code"]`);
    await P.waitForSelector(`${W('notes')} [data-v4-agent]:not([hidden]) [data-agent-input]`, { timeout: 8000 }).catch(() => {});
    await settle(P, 600);
    const ag0 = await P.$eval(W('notes'), (w) => { const a = w.querySelector('[data-v4-agent]'); return a ? { vis: !a.hidden && a.getBoundingClientRect().width > 0, inSameWindow: a.closest('.shell-window') === w, text: a.innerText.slice(0, 200), sync: /synchronisiert/i.test(a.innerText) } : null; });
    rec('M-05', 'Agent als Seitenleiste in der Code-Ansicht desselben Fensters (kein eigenes App-Fenster)', ag0?.vis && ag0.inSameWindow, ag0);
    await P.evaluate(() => { window.__lab.kernelOpts.codingDelayMs = 300; });
    const draft0 = await P.evaluate(async () => (await window.__lab.kernel.source('notes').draft.info()) ?? null);
    await mclick(P, `${W('notes')} [data-agent-input]`);
    await P.keyboard.type('Überschrift fett setzen');
    await P.screenshot({ path: shotPath('M-05', 'agent-auftrag-1280') });
    await P.keyboard.press('Enter');
    const agS = () => P.evaluate((o) => { const s = window.__lab.frame.shellHandle().win(o).code?.view?.agent?.state; return s ? { turns: s.turns.map((t) => ({ state: t.state, prompt: t.prompt, files: t.files ?? t.changed_files ?? null })), last: s.log?.at(-1) && { type: s.log.at(-1).type, status: s.log.at(-1).status } } : null; }, o);
    let ag1 = await agS();
    for (let i = 0; i < 40 && !(ag1?.turns || []).some((t) => /done|complete|finished|succeeded/.test(t.state || '')); i++) { await settle(P, 250); ag1 = await agS(); }
    await settle(P, 500);
    const draft1 = await P.evaluate(async () => (await window.__lab.kernel.source('notes').draft.info()) ?? null);
    await P.screenshot({ path: shotPath('M-05', 'agent-ergebnis-1280') });
    const live = await P.evaluate(() => window.__lab.kernel.apps.get().find((a) => a.id === 'notes')?.active_ver);
    rec('M-05', 'Auftrag per Enter: ctox.coding.turn, Turn endet, Ergebnis im Entwurf (Rev steigt), laufende App-Version unverändert', ag1?.last?.type === 'ctox.coding.turn' && ag1.turns.some((t) => /done|complete|finished|succeeded/.test(t.state || '')) && String(draft1?.rev) !== String(draft0?.rev), { ag1, rev0: draft0?.rev, rev1: draft1?.rev, live, diffVisible: await P.$eval(W('notes'), (w) => !!w.querySelector('.monaco-diff-editor, [data-v4-diff]:not([hidden]), .v4-src__diff')) });
    rec('M-03', 'Weg „Code ansehen“ → Agent: Seitenleiste offen, kein leerer Zustand „Daten werden synchronisiert“', ag0?.vis && !ag0.sync && ag1?.turns?.length > 0, { sync: ag0?.sync }); // v4-allow: code (Laborprüfung: App-Titel bzw. zitierter Owner-Wortlaut)
    // Coding Agents (echte App): Übersicht der Läufe, Klick auf einen Lauf → Code-Ansicht mit Agent
    await mclick(P, `${W('notes')} .shell-window-v2-icon`); await settle(P, 200);
    await mclick(P, `${W('notes')} [data-v4-appmenu="app"]`, { must: false }); await settle(P, 300);
    await P.evaluate((o) => window.__lab.frame.shellHandle().win(o).handle.minimize?.(), o);
    const ca = await open('coding-agents', { x: 120, y: 60, w: 620, h: 460 });
    await P.waitForSelector(`${W('coding-agents')} [data-ca-run]`, { timeout: 6000 }).catch(() => {});
    await settle(P, 600);
    const caInfo = await P.$eval(W('coding-agents'), (w) => ({ runs: w.querySelectorAll('[data-ca-run]').length, sync: /synchronisiert/i.test(w.innerText), text: w.querySelector('.v4-app-root')?.innerText.slice(0, 300) }));
    await P.screenshot({ path: shotPath('M-03', 'coding-agents-uebersicht-1280') });
    const runClick = await mclick(P, `${W('coding-agents')} [data-ca-run][data-app="notes"]`, { must: false });
    const ok7 = await P.waitForSelector(`${W('notes')}[data-v4-mode="code"] [data-v4-agent="notes"]:not([hidden])`, { timeout: 6000 }).then(() => true, () => false);
    await settle(P, 500);
    await P.screenshot({ path: shotPath('M-03', 'lauf-geklickt-code-mit-agent-1280') });
    rec('M-03', 'Coding Agents listet den Lauf; echter Klick darauf öffnet Notizen in der Code-Ansicht mit Agent (kein Leerlauf)', caInfo.runs >= 1 && !caInfo.sync && runClick.ok && ok7, { caInfo, runClick: runClick.ok, ok7 });

    // ---- M-06 Kreislauf: Melden per App-Menü (echte Maus) → Reiter „Meldungen“ → Agent beauftragen ----
    await P.evaluate((o) => window.__lab.frame.shellHandle().closeWindow(o), ca);
    // Pin 4b0d0ac: Eigentümer ctox ist seit dem Seitenaufbau aktiv (Reiter „Meldungen“ liest business_module_reports).
    await settle(P, 300);
    await mclick(P, `${W('notes')} .shell-window-v2-icon`); await settle(P, 200);
    await mclick(P, `${W('notes')} [data-v4-appmenu="app"]`, { must: false }); await settle(P, 300);
    await mclick(P, `${W('notes')} .shell-window-v2-icon`); await settle(P, 200);
    const menu3 = await P.$$eval(`${W('notes')} [data-v4-appmenu]`, (els) => els.map((e) => e.textContent.trim()));
    await mclick(P, `${W('notes')} [data-v4-appmenu="report-bug"]`);
    await P.waitForSelector(`${W('notes')} [data-v4-sheet]`, { timeout: 3000 });
    await mclick(P, `${W('notes')} [data-v4-sheet-input]`);
    await P.keyboard.type('Liste springt nach dem Speichern nach oben');
    await P.screenshot({ path: shotPath('M-06', 'melden-blatt-1280') });
    await mclick(P, `${W('notes')} [data-v4-sheet-ok]`);
    await settle(P, 200);
    await mclick(P, `${W('notes')} [data-v4-sheet-input]`);
    await P.keyboard.type('Liste bleibt stehen');
    await mclick(P, `${W('notes')} [data-v4-sheet-ok]`);
    await settle(P, 600);
    const stored = await P.evaluate(async () => {
      const api = window.__lab.kernel.appApi('ctox', '1'); // Eigentümer liest (Pin 4b0d0ac)
      const lq = api.query('business_module_reports', {});
      for (let i = 0; i < 30 && !(lq.get() || []).length; i++) await new Promise((r) => setTimeout(r, 50));
      const r = (lq.get() || []).at(-1) || {};
      return { id: r.id, module_id: r.module_id, kind: r.kind, summary: r.summary, expected: r.expected, status: r.status, app: r.client_context?.app_id, window_image: r.client_context?.window_image ?? null, ctx: Object.keys(r.client_context ?? {}).sort() };
    });
    rec('M-06', 'Melden aus dem Fenster per App-Menü (echte Maus): Meldung mit App und Erwartung gespeichert', stored.module_id === 'notes' && stored.summary && stored.expected, { menu3, stored });
    // M-06 entschieden (docs/owner-maengel.md, Sa 17.10.): KEIN Fensterbild – Bildschirmfreigabe wäre ein Dialog außerhalb
    // der App; stattdessen strukturierter Kontext (App, Version, Fenstergröße …) in client_context (v4/shell/report.js).
    rec('M-06', 'Meldung trägt strukturierten Kontext statt Fensterbild (Entscheidung M-06)', stored.window_image === null && stored.app === 'notes' && stored.ctx.includes('app_ver'), { window_image: stored.window_image, ctx: stored.ctx });
    await mclick(P, `${W('notes')} .shell-window-v2-icon`); await settle(P, 200);
    await mclick(P, `${W('notes')} [data-v4-appmenu="code"]`);
    await P.waitForSelector(`${W('notes')} [data-src-tab="reports"]`, { timeout: 8000 }).catch(() => {});
    await settle(P, 600);
    await mclick(P, `${W('notes')} [data-src-tab="reports"]`);
    await settle(P, 400);
    const rows = await P.$$eval(`${W('notes')} [data-v4-reports] [data-report-id]`, (els) => els.map((e) => ({ id: e.dataset.reportId, title: e.querySelector('.v4-src__report-title')?.textContent, st: e.querySelector('.v4-src__report-status')?.textContent, assign: !!e.querySelector('[data-report-assign]') })));
    await P.screenshot({ path: shotPath('M-06', 'reiter-meldungen-1280') });
    rec('M-06', 'Reiter „Meldungen“ in der Code-Ansicht zeigt die Meldung mit „Agent beauftragen“', rows.some((r) => r.id === stored.id && r.assign), rows);
    const as = await mclick(P, `${W('notes')} [data-report-assign="${stored.id}"]`, { must: false });
    await P.waitForSelector(`${W('notes')} [data-v4-reports] [data-report-id="${stored.id}"][data-status="running"], ${W('notes')} [data-v4-reports] [data-report-id="${stored.id}"][data-status="done"]`, { timeout: 6000 }).catch(() => {});
    await settle(P, 2000);
    const rowAfter = await P.$eval(`${W('notes')} [data-v4-reports] [data-report-id="${stored.id}"]`, (e) => ({ status: e.dataset.status, st: e.querySelector('.v4-src__report-status')?.textContent, turn: e.querySelector('[data-report-turn]')?.dataset.reportTurn ?? null })).catch(() => null);
    await P.screenshot({ path: shotPath('M-06', 'agent-beauftragt-1280') });
    rec('M-06', '„Agent beauftragen“ (echter Klick): Meldung in Arbeit, Lauf verlinkt', as.ok && rowAfter?.turn, rowAfter);
    // Veröffentlichen schlägt Versionsnotiz „behebt Meldung #N“ vor
    await P.waitForFunction((sel) => { const b = document.querySelector(`${sel} .v4-src__btn--primary`); return b && !b.disabled; }, W('notes'), { timeout: 8000 }).catch(() => {});
    const pub = await P.$(`${W('notes')} .v4-src__btn--primary`);
    let note = null;
    if (pub) {
      await mclick(P, pub, { must: false });
      await settle(P, 600);
      note = await P.evaluate(() => { const s = document.querySelector('[data-v4-sheet]'); return s ? { title: s.querySelector('.v4-sheet-title')?.textContent, text: s.querySelector('.v4-sheet-text')?.textContent, val: s.querySelector('[data-v4-sheet-input]')?.value } : null; });
      await P.screenshot({ path: shotPath('M-06', 'veroeffentlichen-notiz-1280') });
      await P.keyboard.press('Escape');
    }
    rec('M-06', 'Veröffentlichen schlägt Versionsnotiz „behebt Meldung #…“ vor', /behebt Meldung #/.test(note?.val || ''), { pub: !!pub, note });
    rec('M-02/03/05/06', 'ohne Seitenfehler', perr.length === 0, perr.slice(0, 4));
    await ctx.close();
  }

  // ================= M-07 Taskleiste: ein Eintrag je App, Crew gruppiert, wächst nicht =================
  if (want('M07')) {
    for (const w of [1280, 420]) {
      const { ctx, P, open } = await lab({ w, crew: true });
      for (const [id, g] of [['notes', { x: 30, y: 30, w: 520, h: 340 }], ['tasks', { x: 580, y: 30, w: 520, h: 340 }], ['ledger', { x: 300, y: 300, w: 520, h: 340 }], ['crm', { x: 200, y: 120, w: 520, h: 340 }]]) await open(id, g);
      const crewWins = [];
      for (let i = 0; i < 6; i++) crewWins.push(await open(CREW[i % 3][0], { x: 60 + i * 40, y: 120 + i * 30, w: 420, h: 300 }));
      await settle(P, 500);
      const bar = await P.evaluate(() => {
        const el = document.querySelector('[data-module-tabs]'); const tr = el.getBoundingClientRect();
        const all = [...el.querySelectorAll('.module-tab[data-v4-task-group]')];
        const more = el.querySelector(':scope > details.module-overflow');
        return { barW: Math.round(tr.width), entries: all.map((n) => ({ g: n.dataset.v4TaskGroup, label: n.querySelector('.module-tab-label')?.textContent, count: n.querySelector('[data-v4-count]')?.hidden ? 0 : n.querySelector('[data-v4-count]')?.textContent, inMore: !!n.closest('details') })),
          more: more?.querySelector('summary')?.textContent || '', overflowX: document.documentElement.scrollWidth > innerWidth, fits: all.filter((n) => !n.closest('details')).every((n) => { const r = n.getBoundingClientRect(); return r.right <= tr.right + 0.5; }) };
      });
      await P.screenshot({ path: shotPath('M-07', `leiste-${w}`) });
      const crewE = bar.entries.find((e) => e.g === 'crew');
      rec('M-07', `${w}: 10 Fenster → 5 Einträge (je App einer), 6 Crew-Chats unter „Crew“ mit Zahl 6, Leiste passt`, bar.entries.length === 5 && crewE?.count === '6' && bar.fits && !bar.overflowX, bar);
      if (w === 1280) {
        const seen = new Set();
        for (let i = 0; i < 6; i++) {
          await mclick(P, '[data-v4-task-group="crew"]');
          await settle(P, 250);
          seen.add(await P.evaluate(() => document.querySelector('.shell-window.is-focused')?.id));
        }
        rec('M-07', 'echter Klick auf „Crew“ wechselt reihum durch die 6 Chats', seen.size === 6, { distinct: seen.size });
        await mclick(P, '[data-v4-task-group="crew"]', { button: 'right' });
        await settle(P, 300);
        const list = await P.evaluate(() => [...document.querySelectorAll('[data-v4-task-menu] [data-v4-task-menu-win]')].filter((b) => b.checkVisibility()).map((b) => b.textContent.trim()));
        await P.screenshot({ path: shotPath('M-07', 'rechtsklick-crew-1280') });
        rec('M-07', 'Rechtsklick auf „Crew“ zeigt die Fensterliste', list.length >= 6, { list });
        await P.keyboard.press('Escape');
      }
      await ctx.close();
    }
  }

  // ================= M-08 Statuspunkt bei Sync-Störung (Labor: Rückstand/Verbindung im Stub) =================
  if (want('M08')) {
    const { ctx, P, open } = await lab();
    await open('notes', { x: 40, y: 60, w: 560, h: 360 });
    await settle(P, 800);
    const dot = () => P.evaluate(() => { const b = document.querySelector('[data-v4-topbar] [data-v4-sync]'); const p = document.createElement('span'); p.style.color = 'var(--success)'; b.append(p); const green = getComputedStyle(p).color; p.remove(); return { state: b.dataset.syncState ?? null, green: getComputedStyle(b.querySelector('.shell-sync-dot')).backgroundColor === green, label: b.querySelector('[data-sync-state-label]')?.textContent || '' }; });
    const d0 = await dot();
    const hasBacklog = await P.evaluate(() => typeof window.__lab.kernel.sim.setBacklog === 'function');
    if (hasBacklog) await P.evaluate(() => window.__lab.kernel.sim.setBacklog(5));
    await settle(P, 2600);
    const d1 = await dot();
    await P.screenshot({ path: shotPath('M-08', 'rueckstand-1280') });
    rec('M-08', 'Labor: Rückstand im Stub → Punkt nicht grün, mit Text', hasBacklog && d0.green === true && !d1.green && !!d1.label, { d0, d1 });
    await ctx.close();
  }

  // ================= M-09 Icons: eine Quelle, kein Wechsel beim Laden =================
  if (want('M09')) {
    const { ctx, P, open } = await lab();
    await P.evaluate(() => {
      window.__iconHist = [];
      new MutationObserver((ms) => { for (const m of ms) { const t = m.target; if (t.matches?.('[data-window-app-icon]') && m.attributeName === 'src') window.__iconHist.push([t.closest('.shell-window')?.dataset.windowApp, (t.getAttribute('src') || '').slice(0, 40)]); } })
        .observe(document.body, { subtree: true, attributes: true, attributeFilter: ['src'] });
    });
    // Öffnen per Startmenü (echte Maus)
    await mclick(P, '[data-shell-start]');
    await settle(P, 300);
    const tile = await P.$('[data-v4-launcher] [data-v4-app="studio"]');
    await mclick(P, tile);
    await P.waitForSelector(`${W('studio')} [data-window-app-icon]`, { timeout: 5000 });
    await settle(P, 1500);
    await open('crm', { x: 700, y: 60, w: 520, h: 360 });
    await settle(P, 1200);
    const icons = await P.evaluate(() => Object.fromEntries(['studio', 'crm'].map((id) => {
      const l = document.querySelector(`[data-v4-app="${id}"] img`)?.getAttribute('src') || '';
      const wimg = document.querySelector(`.shell-window[data-window-app="${id}"] [data-window-app-icon]`);
      const tab = [...document.querySelectorAll('[data-module-tabs] img')].map((i) => i.getAttribute('src')).includes(l);
      return [id, { same: !!l && l === wimg?.getAttribute('src') && tab, loaded: wimg?.naturalWidth > 0, generated: l.startsWith('data:image/svg+xml'), monogram: !!document.querySelector(`.shell-window[data-window-app="${id}"] .is-fallback`) }];
    })));
    const hist = await P.evaluate(() => window.__iconHist);
    await P.screenshot({ path: shotPath('M-09', 'icons-1280') });
    const perApp = {}; for (const [a, s] of hist) (perApp[a] ||= new Set()).add(s);
    rec('M-09', 'Launcher, Reiter und Fenster zeigen dasselbe Bild; App ohne Icon (Studio) bekommt ein Ersatz-Icon, kein Monogramm; geladen', icons.studio.same && icons.crm.same && icons.studio.loaded && icons.crm.loaded && !icons.studio.monogram, icons);
    rec('M-09', 'Icon wechselt beim Öffnen nicht (je Fenster höchstens eine src)', Object.values(perApp).every((s) => s.size <= 1), Object.fromEntries(Object.entries(perApp).map(([k, v]) => [k, [...v]])));
    // Ersatz-Icon vergrößert ansehen
    const ir = await rect(P, `${W('studio')} .shell-window-v2-icon`);
    await P.screenshot({ path: shotPath('M-09', 'studio-ersatz-icon-zoom'), clip: { x: Math.max(0, ir.l - 10), y: Math.max(0, ir.t - 10), width: 220, height: 140 } });
    await ctx.close();
  }

  // ================= M-10/M-11/M-12/M-16 Studio-Nachbau: Zonen, Vollbild, Sprache =================
  if (want('M10') || want('M11') || want('M12') || want('M16')) {
    for (const w of [1280, 420]) {
      const { ctx, P, open, perr } = await lab({ w });
      const g = w === 1280 ? { x: 160, y: 120, w: 900, h: 480 } : null;
      const o = await open('studio', g);
      await settle(P, 900);
      const z = await P.$eval(W('studio'), (win) => {
        const ic = win.querySelector('.shell-window-v2-icon').getBoundingClientRect();
        const ctl = win.querySelector('[data-window-layout-trigger]')?.closest('.shell-window-controls, [data-window-actions]')?.getBoundingClientRect() || win.querySelector('[data-window-layout-trigger]').getBoundingClientRect();
        const box = (r) => [Math.round(r.left), Math.round(r.top), Math.round(r.right), Math.round(r.bottom)];
        const inter = (r, q) => r.width > 0 && r.left < q.right && r.right > q.left && r.top < q.bottom && r.bottom > q.top;
        const els = [...win.querySelectorAll('.v4-app-root button, .v4-app-root select, .v4-app-root input')];
        return { icon: box(ic), controls: box(ctl), under: els.filter((e) => inter(e.getBoundingClientRect(), ic) || inter(e.getBoundingClientRect(), ctl)).map((e) => e.outerHTML.slice(0, 50)), tools: els.map((e) => ({ k: Object.keys(e.dataset)[0], b: box(e.getBoundingClientRect()) })) };
      });
      await P.screenshot({ path: shotPath('M-10', `studio-zonen-${w}`) });
      // jede Werkzeugleisten-Taste per echter Maus erreichbar
      const hits = {};
      for (const k of ['settings', 'png', 'lang', 'full']) {
        const h = await P.$(`${W('studio')} [data-studio-${k}]`);
        hits[k] = h ? await h.evaluate((el) => { const b = el.getBoundingClientRect(); const x = b.left + b.width / 2; const y = b.top + b.height / 2; const t = document.elementFromPoint(x, y); return !!t && (t === el || el.contains(t)); }) : null;
      }
      rec('M-10', `${w}: kein App-Bedienelement unter Icon-Header oder Knopfcluster; jede Taste per Treffertest frei`, z.under.length === 0 && Object.values(hits).every((v) => v === true), { ...z, hits });
      if (w === 1280) {
        // Layout-Trigger und Schließen bleiben bedienbar (Gegenrichtung: Shell-Knöpfe nicht von App verdeckt)
        const trig = await P.$eval(`${W('studio')} [data-window-layout-trigger]`, (el) => { const b = el.getBoundingClientRect(); const t = document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2); return !!t && (t === el || el.contains(t)); });
        rec('M-16', 'Shell-Knöpfe über der App bleiben frei treffbar, App platziert darunter nichts', trig && z.under.length === 0, { trig });
        // M-11 Vollbild: App-Knopf (echte Maus) → Element-Vollbild über die Shell; Esc
        const full = await mclick(P, `${W('studio')} [data-studio-full]`);
        await settle(P, 500);
        const fs1 = await P.evaluate(() => !!document.fullscreenElement?.closest('.shell-window[data-window-app="studio"]'));
        await P.screenshot({ path: shotPath('M-11', 'app-knopf-vollbild-1280') });
        await P.evaluate(() => document.exitFullscreen?.().catch(() => {}));
        await settle(P, 400);
        // Layout-Menü: gibt es dort „Vollbild“?
        await mhover(P, `${W('studio')} [data-window-layout-trigger]`);
        await P.waitForSelector(`${W('studio')} [data-window-layout-menu]:not([hidden])`, { timeout: 3000 }).catch(() => {});
        await settle(P, 300);
        const items = await P.$$eval(`${W('studio')} [data-window-layout-menu] [data-window-layout-control]`, (e) => e.map((b) => `${b.dataset.windowLayoutControl}:${b.getAttribute('aria-label')}`));
        await P.screenshot({ path: shotPath('M-11', 'layout-menue-1280') });
        rec('M-11', 'App-eigener Vollbild-Knopf (echter Klick) wirkt über die Shell (Element-Vollbild)', full.ok && fs1, { fs1 });
        rec('M-11', 'Vollbild im Layout-Menü (Bedienkonzept: „über das Layout-Menü und mit Strg+Cmd+F bzw. F11“)', items.some((s) => /full|vollbild/i.test(s)), { items });
        await P.mouse.move(5, 790);
        await settle(P, 200);
        await mclick(P, `${W('studio')} [data-studio-title]`);
        await P.keyboard.press('F11');
        await settle(P, 500);
        const f11 = await P.evaluate(() => !!document.fullscreenElement?.closest('.shell-window[data-window-app="studio"]'));
        await P.keyboard.press('F11');
        await settle(P, 400);
        const f11off = await P.evaluate(() => !document.fullscreenElement);
        rec('M-11', 'F11 schaltet Vollbild für das fokussierte Fenster ein und aus', f11 && f11off, { f11, f11off });
        // M-12 Sprache: Shell-Einstellung Sprache wirkt; die Lab-App hat (als Nachbau) noch eigenen DE/EN-Umschalter
        const ownLang = !!(await P.$(`${W('studio')} [data-studio-lang]`));
        await mclick(P, '[data-v4-settings-button]');
        await settle(P, 400);
        const langCtl = await P.$$eval('[data-v4-settings-panel] select, [data-v4-settings-panel] [role=radio], [data-v4-settings-panel] input', (e) => e.map((x) => `${x.tagName}:${x.getAttribute('name') || x.getAttribute('aria-label') || ''}:${x.value}`));
        await P.screenshot({ path: shotPath('M-12', 'einstellungen-sprache-1280') });
        rec('M-12', 'Sprache in den Shell-Einstellungen vorhanden', langCtl.length > 0, { langCtl, labAppOwnSwitcher: ownLang });
        await P.keyboard.press('Escape');
      }
      rec('M-10', `${w}: ohne Seitenfehler`, perr.length === 0, perr.slice(0, 3));
      await ctx.close();
    }
  }

  // ================= M-14 Meldungen als Badge am Icon, aufklappbar =================
  if (want('M14')) {
    const { ctx, P, open } = await lab();
    const o = await open('ledger', { x: 120, y: 80, w: 640, h: 420 });
    await settle(P, 600);
    await P.evaluate((o) => { const r = window.__lab.frame.shellHandle().win(o); r.host.zone.report({ level: 'warn', code: 'lab.warn', text: 'Bankabgleich verzögert', source: 'app' }); r.host.zone.report({ level: 'error', code: 'lab.err', text: 'Beleg konnte nicht gespeichert werden', source: 'app', action: { label: 'Erneut', run: () => {} } }); }, o);
    await settle(P, 2600);
    const b0 = await P.$eval(W('ledger'), (w) => { const z = w.querySelector('.v4-status-zone'); const b = z?.querySelector('.v4-status-zone__badge'); const i = w.querySelector('.shell-window-v2-icon').getBoundingClientRect(); const br = b?.getBoundingClientRect(); return { shown: !!z && !z.hidden, n: b?.textContent, level: z?.dataset.level, atIcon: !!br && br.left < i.right + 4 && br.top < i.bottom + 4 && br.right > i.left, listOpen: !w.querySelector('.v4-status-zone__list').hidden, infoboxInContent: !!w.querySelector('.v4-app-root .ctox-alert, .v4-app-root [role=alert]') }; });
    await P.screenshot({ path: shotPath('M-14', 'badge-zu-1280') });
    await mclick(P, `${W('ledger')} .v4-status-zone__badge`);
    await settle(P, 300);
    const b1 = await P.$eval(W('ledger'), (w) => { const l = w.querySelector('.v4-status-zone__list'); return { listOpen: !l.hidden, rows: [...l.querySelectorAll('.v4-status-zone__item')].map((r) => r.innerText.replace(/\s+/g, ' ').trim()) }; });
    await P.screenshot({ path: shotPath('M-14', 'badge-auf-1280') });
    rec('M-14', 'Badge mit Zahl am App-Icon (rot bei Fehler), Liste erst nach Klick; Einträge mit Aktion', b0.shown && b0.n === '2' && b0.level === 'error' && b0.atIcon && !b0.listOpen && b1.listOpen && b1.rows.length === 2, { b0, b1 });
    await ctx.close();
  }

  // ================= M-17 Kantenfarben aus dem Logo (Pixel, ΔE) – Lab-Apps und echte App-Icons =================
  if (want('M17')) {
    const { ctx, P, open } = await lab();
    // Echte Icons aus dem v4-Manifest (v4/manifests/<id>.manifest.json: icon_asset = Raster wie Shell-V2, dazu icon_svg).
    const real = {};
    for (const id of ['reports', 'credentials', 'news', 'mail', 'coding-agents']) {
      const l = JSON.parse(fs.readFileSync(path.resolve(here, '../../manifests', `${id}.manifest.json`), 'utf8')).layout || {};
      real[id] = { ...(l.icon_asset ? { icon_asset: l.icon_asset } : {}), ...(l.icon_svg ? { icon_svg: l.icon_svg } : {}) };
    }
    await P.route(/\/~a\/real-[a-z-]+@/, (route) => {
      const u = new URL(route.request().url()).pathname;
      const m = /\/~a\/real-([a-z-]+)@[^/]+\/(.+)$/.exec(u);
      if (m && /\.(jpg|png|svg)$/.test(m[2])) { const f = path.join(MOD, m[1], m[2]); if (fs.existsSync(f)) { route.fulfill({ status: 200, contentType: m[2].endsWith('.svg') ? 'image/svg+xml' : m[2].endsWith('.png') ? 'image/png' : 'image/jpeg', body: fs.readFileSync(f) }); return; } route.fulfill({ status: 404 }); return; }
      route.fulfill({ status: 200, contentType: 'text/javascript', body: 'export function mount(root){root.innerHTML="<div style=\\"padding:20px;font:13px system-ui\\">Inhalt</div>";return{unmount(){root.replaceChildren()}}}' });
    });
    await P.evaluate(async (real) => {
      for (const [id, icon] of Object.entries(real)) await window.__lab.kernel.sim.publishAppVersion(`real-${id}`, '1.0.0', { title: id, layout: { ...icon, category: 'Labor', default_width: 420, default_height: 300 }, styles: [] });
    }, real);
    const ids = ['notes', 'crm', 'studio', ...Object.keys(real).map((k) => `real-${k}`)];
    const rows = [];
    const toLab = ([r, g, b]) => { const f = (v) => { v /= 255; return v > 0.04045 ? ((v + 0.055) / 1.055) ** 2.4 : v / 12.92; }; const [R, G, B] = [f(r), f(g), f(b)]; const X = (R * 0.4124 + G * 0.3576 + B * 0.1805) / 0.95047; const Y = R * 0.2126 + G * 0.7152 + B * 0.0722; const Z = (R * 0.0193 + G * 0.1192 + B * 0.9505) / 1.08883; const h = (t) => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116); return [116 * h(Y) - 16, 500 * (h(X) - h(Y)), 200 * (h(Y) - h(Z))]; };
    const dE = (a, b) => { const [p, q] = [toLab(a), toLab(b)]; return +Math.hypot(p[0] - q[0], p[1] - q[1], p[2] - q[2]).toFixed(1); };
    for (const id of ids) {
      const ow = await open(id, { x: 200, y: 140, w: 520, h: 360 });
      await settle(P, 1200);
      const g = await P.$eval(W(id), (w) => { const wr = w.getBoundingClientRect(); const ir = w.querySelector('.shell-window-v2-icon').getBoundingClientRect(); return { wl: wr.left, wt: wr.top, il: ir.left, it: ir.top, ir: ir.right, ib: ir.bottom, pal: w.dataset.shellV2PaletteSource || '' }; });
      const png = await P.screenshot();
      const px = await P.evaluate(async ([b64, pts]) => { const img = new Image(); img.src = `data:image/png;base64,${b64}`; await img.decode(); const c = document.createElement('canvas'); c.width = img.naturalWidth; c.height = img.naturalHeight; const x = c.getContext('2d'); x.drawImage(img, 0, 0); return pts.map(([a, b]) => [...x.getImageData(Math.round(a), Math.round(b), 1, 1).data].slice(0, 3)); },
        [png.toString('base64'), [[g.ir - 2, g.it + 1], [g.ir + 2, g.wt + 1], [g.il + 1, g.ib - 2], [g.wl + 1, g.ib + 2], [g.ir + 60, g.wt + 1]]]);
      // PAL: Naht am deckenden Logopixel messen (v4/tools/geometry/g9.mjs); das Eckpixel sah bei runden Ecken den Halter.
      const { g9Pixel } = await import('../../tools/geometry/g9.mjs');
      const gp = await g9Pixel(P, W(id), { label: id });
      rows.push({ id, palette: g.pal, top: gp.row ? +gp.row.top.toFixed(1) : 99, left: gp.row ? +gp.row.left.toFixed(1) : 99, corner: { top: dE(px[0], px[1]), left: dE(px[2], px[3]) }, logoTop: px[0], edgeTop: px[1], edgeFar: px[4], ...(gp.skip ? { skip: gp.skip } : {}) });
      await P.screenshot({ path: shotPath('M-17', `kante-${id}`), clip: { x: Math.max(0, g.wl - 8), y: Math.max(0, g.wt - 8), width: 560, height: 200 } });
      await P.evaluate((o) => window.__lab.frame.shellHandle().closeWindow(o), ow);
      await settle(P, 300);
    }
    rec('M-17', 'ΔE Logo-Rand ↔ Kante oben/links < 12 je App (Lab-Apps und echte v4-Icons reports/credentials/news/mail/coding-agents), Palette aus Icon berechnet', rows.every((r) => r.top < 12 && r.left < 12 && r.palette !== 'manifest'), rows.map(({ id, palette, top, left, logoTop, edgeTop }) => ({ id, palette, top, left, logoTop, edgeTop })));
    await ctx.close();
  }

  // ================= M-17 Zoom: Logo mit runden Ecken (notes, rx 14) gegen vollflächiges Raster-Icon (reports) =================
  if (want('M17z')) {
    const ctx = await browser.newContext({ viewport: { width: 900, height: 600 }, deviceScaleFactor: 3, locale: 'de-DE' });
    const P = await ctx.newPage();
    await P.goto(`${base}/v4/frame/lab/stub-lab.html?inst=mgz-${Date.now().toString(36)}`);
    await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
    await P.route(/\/~a\/real-reports@/, (route) => { const u = new URL(route.request().url()).pathname; if (u.endsWith('.jpg')) { route.fulfill({ status: 200, contentType: 'image/jpeg', body: fs.readFileSync(path.join(MOD, 'reports/assets/icon.jpg')) }); return; } route.fulfill({ status: 200, contentType: 'text/javascript', body: 'export function mount(root){root.innerHTML="<div style=\\"padding:20px\\">Inhalt</div>";return{unmount(){root.replaceChildren()}}}' }); });
    await P.evaluate(() => window.__lab.kernel.sim.publishAppVersion('real-reports', '1.0.0', { title: 'Bugs & Features', layout: { icon_asset: 'assets/icon.jpg', category: 'Labor' }, styles: [] })); // v4-allow: en (Laborprüfung: App-Titel)
    await P.waitForSelector('[data-v4-app="real-reports"]', { state: 'attached' });
    for (const id of ['notes', 'real-reports']) {
      const ow = await P.evaluate(async (id) => { const hh = window.__lab.frame.shellHandle(); const o = hh.openWindow(id, { geometry: { x: 120, y: 120, w: 500, h: 300 } }); await hh.win(o).host.ready; return o; }, id);
      await settle(P, 1500);
      const g = await P.$eval(W(id), (w) => { const r = w.getBoundingClientRect(); const s = getComputedStyle(w); return { l: r.left, t: r.top, top: s.getPropertyValue('--shell-v2-frame-top-joint'), left: s.getPropertyValue('--shell-v2-frame-left-joint'), src: w.dataset.shellV2PaletteSource }; });
      await P.screenshot({ path: shotPath('M-17', `zoom-${id}`), clip: { x: g.l - 30, y: g.t - 30, width: 200, height: 160 } });
      // Deckendes Logopixel (links im Icon, 4 px eingerückt, 20 px über der Unterkante – oberhalb des Eckradius) gegen die linke Kante direkt unter dem Icon.
      const ib = await P.$eval(`${W(id)} .shell-window-v2-icon`, (e) => { const r = e.getBoundingClientRect(); return { l: r.left, t: r.top, b: r.bottom, h: r.height }; });
      const png = await P.screenshot({ clip: { x: 0, y: 0, width: 900, height: 600 } });
      const px = await P.evaluate(async ([b64, pts]) => { const img = new Image(); img.src = `data:image/png;base64,${b64}`; await img.decode(); const c = document.createElement('canvas'); c.width = img.naturalWidth; c.height = img.naturalHeight; const x = c.getContext('2d'); x.drawImage(img, 0, 0); return pts.map(([a, b]) => [...x.getImageData(Math.round(a * 3), Math.round(b * 3), 1, 1).data].slice(0, 3)); }, [png.toString('base64'), [[ib.l + 4, ib.b - 20], [g.l + 1.5, ib.b + 6], [ib.l + 1, ib.b - 1]]]);
      const toLab = ([r, gg, b]) => { const f = (v) => { v /= 255; return v > 0.04045 ? ((v + 0.055) / 1.055) ** 2.4 : v / 12.92; }; const [R, G, B] = [f(r), f(gg), f(b)]; const X = (R * 0.4124 + G * 0.3576 + B * 0.1805) / 0.95047; const Y = R * 0.2126 + G * 0.7152 + B * 0.0722; const Z = (R * 0.0193 + G * 0.1192 + B * 0.9505) / 1.08883; const h = (t) => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116); return [116 * h(Y) - 16, 500 * (h(X) - h(Y)), 200 * (h(Y) - h(Z))]; };
      const de = (a, b) => { const [p, q] = [toLab(a), toLab(b)]; return +Math.hypot(p[0] - q[0], p[1] - q[1], p[2] - q[2]).toFixed(1); };
      const d = de(px[0], px[1]);
      rec('M-17', `Zoom ${id}: deckendes Logopixel ↔ linke Kante unter dem Icon ΔE < 12 (keine Naht)`, d < 12, { ...g, logo: px[0], edge: px[1], cornerPixel: px[2], dE: d });
      await P.evaluate((o) => window.__lab.frame.shellHandle().closeWindow(o), ow);
      await settle(P, 300);
    }
    await ctx.close();
  }

  // ================= M-18 Desktop minimiert alle, zweiter Klick stellt her =================
  if (want('M18')) {
    for (const w of [1280, 420]) {
      const { ctx, P, open } = await lab({ w });
      for (const [id, g] of [['notes', { x: 30, y: 30, w: 520, h: 340 }], ['tasks', { x: 580, y: 30, w: 520, h: 340 }], ['crm', { x: 300, y: 300, w: 520, h: 340 }]]) await open(id, g);
      await settle(P, 500);
      const vis = () => P.$$eval('.shell-window', (els) => els.filter((e) => getComputedStyle(e).display !== 'none' && e.getBoundingClientRect().width > 0 && !e.classList.contains('is-minimized')).map((e) => e.dataset.windowApp));
      const v0 = await vis();
      await mclick(P, '[data-shell-start]');
      await settle(P, 300);
      const btn = await P.$('[data-v4-launcher-desktop]');
      const label = btn ? await btn.evaluate((b) => (b.getAttribute('aria-label') || b.textContent).trim()) : null;
      await P.screenshot({ path: shotPath('M-18', `startmenue-${w}`) });
      const c1 = await mclick(P, '[data-v4-launcher-desktop]', { must: false });
      await settle(P, 700);
      const v1 = await vis();
      await P.screenshot({ path: shotPath('M-18', `desktop-frei-${w}`) });
      await mclick(P, '[data-shell-start]');
      await settle(P, 300);
      const c2 = await mclick(P, '[data-v4-launcher-desktop]', { must: false });
      await settle(P, 700);
      const v2 = await vis();
      await P.screenshot({ path: shotPath('M-18', `wiederhergestellt-${w}`) });
      rec('M-18', `${w}: „${label}“ (echter Klick) minimiert alle Fenster, zweiter Klick stellt sie wieder her`, c1.ok && c2.ok && v0.length >= (w === 1280 ? 3 : 1) && v1.length === 0 && v2.length === v0.length, { v0, v1, v2, c1: c1.ok, c2: c2.ok });
      await ctx.close();
    }
  }

  // ================= M-20 Zahnrad links vom Avatar, Konto-Panel oben „Einstellungen“ =================
  if (want('M20')) {
    for (const w of [1280, 420]) {
      const { ctx, P } = await lab({ w, extra: false });
      await settle(P, 400);
      const top = await P.evaluate(() => { const g = document.querySelector('[data-v4-settings-button]'); const a = document.querySelector('[data-v4-topbar] .account-button'); const gr = g?.getBoundingClientRect(); const ar = a?.getBoundingClientRect(); return { gear: !!gr && gr.width > 0 && getComputedStyle(g).display !== 'none', gearLeftOfAvatar: !!gr && !!ar && gr.right <= ar.left + 1, gr: gr && [Math.round(gr.left), Math.round(gr.width)], ar: ar && [Math.round(ar.left), Math.round(ar.width)] }; });
      await P.screenshot({ path: shotPath('M-20', `kopfleiste-${w}`), clip: { x: 0, y: 0, width: w, height: 60 } });
      let viaGear = null;
      if (top.gear) {
        await mclick(P, '[data-v4-settings-button]');
        await settle(P, 400);
        viaGear = await P.$eval('[data-v4-settings-panel]', (p) => !p.hidden && p.getBoundingClientRect().width > 0);
        await P.screenshot({ path: shotPath('M-20', `einstellungen-${w}`) });
        await P.keyboard.press('Escape'); await settle(P, 300);
        if (await P.$eval('[data-v4-settings-panel]', (p) => !p.hidden)) { const cl = await P.$('[data-v4-settings-panel] [data-close], [data-v4-settings-panel] .drawer-close, [data-v4-settings-panel] button[aria-label="Schließen"]'); if (cl) await mclick(P, cl, { must: false }); await settle(P, 300); }
      }
      await mclick(P, '[data-v4-topbar] .account-button');
      await settle(P, 400);
      const acc = await P.evaluate(() => { const e = document.querySelector('[data-v4-account-settings]'); if (!e) return null; const sib = [...e.parentElement.children].filter((x) => x.checkVisibility()); return { vis: e.checkVisibility(), first: sib[0] === e || e.parentElement.firstElementChild === e, text: e.textContent.trim() }; });
      await P.screenshot({ path: shotPath('M-20', `konto-${w}`) });
      const c = await mclick(P, '[data-v4-account-settings]', { must: false });
      await settle(P, 400);
      const viaAcc = await P.$eval('[data-v4-settings-panel]', (p) => !p.hidden && p.getBoundingClientRect().width > 0);
      rec('M-20', `${w}: ${w >= 560 ? 'Zahnrad links vom Avatar öffnet Einstellungen; ' : ''}Konto-Panel hat oben „Einstellungen“ und öffnet sie`, (w < 560 || (top.gear && top.gearLeftOfAvatar && viaGear)) && acc?.vis && c.ok && viaAcc, { top, viaGear, acc, viaAcc });
      await ctx.close();
    }
  }

  // ================= Echte Apps über das Geometrie-Labor: M-04 coding-agents-Kopf, M-13/M-15 credentials, M-10 credentials-Zonen =================
  const real = async (id, w) => {
    configs.set(id, appConfig(id));
    const ctx = await browser.newContext({ viewport: { width: w, height: 800 }, locale: 'de-DE', deviceScaleFactor: 1 });
    const P = await ctx.newPage();
    const perr = [];
    P.on('pageerror', (e) => perr.push(String(e)));
    await P.goto(`${geoBase}/__geo/page.html?app=${encodeURIComponent(id)}`);
    await P.waitForFunction(() => window.__geoReady === true, null, { timeout: 30000 });
    const owner = await P.evaluate((g) => window.__geo.open(g), w >= 900 ? { x: 40, y: 20, w: Math.min(1100, w - 80), h: 700 } : null);
    await P.waitForTimeout(1500);
    return { ctx, P, perr, owner };
  };
  const zoneAndText = (P, id) => P.$eval(`.shell-window[data-window-app="${id}"]`, (win) => {
    const ic = win.querySelector('.shell-window-v2-icon').getBoundingClientRect();
    const ctl = (win.querySelector('[data-window-layout-trigger]')?.closest('.shell-window-controls, [data-window-actions]') || win.querySelector('[data-window-layout-trigger]')).getBoundingClientRect();
    const inter = (r, q) => r.width > 0 && r.height > 0 && r.left < q.right && r.right > q.left && r.top < q.bottom && r.bottom > q.top;
    const els = [...win.querySelectorAll('.v4-app-root button, .v4-app-root a, .v4-app-root input, .v4-app-root select, .v4-app-root [role=button]')].filter((e) => e.checkVisibility());
    const root = win.querySelector('.v4-app-root');
    const boxes = [...root.querySelectorAll('.ctox-alert, .ctox-notice, [role=alert], [role=note], .notice, .callout, .hint, .ctox-hint, .info-box, [class*="notice"], [class*="banner"], [class*="callout"]')].filter((e) => e.checkVisibility() && e.innerText.trim()).map((e) => `${e.className}: ${e.innerText.trim().slice(0, 90)}`);
    const paras = [...root.querySelectorAll('p, small, .ctox-field-hint, .field-hint, .help, [class*="hint"], [class*="help"], [class*="description"]')].filter((e) => e.checkVisibility() && e.innerText.trim().length > 25).map((e) => e.innerText.trim().slice(0, 90));
    return { under: els.filter((e) => inter(e.getBoundingClientRect(), ic) || inter(e.getBoundingClientRect(), ctl)).map((e) => e.outerHTML.slice(0, 70)), boxes: [...new Set(boxes)].slice(0, 10), longTexts: [...new Set(paras)].slice(0, 12), writeOnly: /write-only|Write-only/i.test(root.innerText), syncText: /Daten werden synchronisiert/i.test(root.innerText) };
  });
  if (want('M04')) {
    const { ctx, P, perr } = await real('coding-agents', 1280);
    const head = await P.$eval('.shell-window[data-window-app="coding-agents"]', (w) => {
      const i = w.querySelector('.shell-window-v2-icon').getBoundingClientRect(); const t = w.querySelector('.ctox-pane-title')?.getBoundingClientRect();
      const own = [...w.querySelectorAll('.v4-app-root img, .v4-app-root svg')].filter((e) => { const r = e.getBoundingClientRect(); return e.checkVisibility() && r.width >= 16 && r.top < i.bottom + 40 && r.left < i.right + 60; }).map((e) => `${e.tagName}.${e.getAttribute('class') || ''}`);
      return { icon: [Math.round(i.left), Math.round(i.top), Math.round(i.width), Math.round(i.height)], title: t && [Math.round(t.left), Math.round(t.top)], titleText: w.querySelector('.ctox-pane-title')?.textContent.trim(), ownIcons: own, ownMark: !!w.querySelector('.coding-agents-rail-mark, .coding-agents-rail-spacer'), iconImg: w.querySelector('[data-window-app-icon]')?.naturalWidth > 0 };
    });
    const zt = await zoneAndText(P, 'coding-agents');
    const srcs = await P.evaluate(() => ({ launcher: document.querySelector('[data-v4-app="coding-agents"] img')?.getAttribute('src') || null, desktop: document.querySelector('.desktop-icon[data-app-id="coding-agents"] img, [data-v4-desktop] [data-app="coding-agents"] img')?.getAttribute('src') || null, tab: document.querySelector('[data-module-tabs] [data-v4-task-group="coding-agents"] img')?.getAttribute('src') || null, win: document.querySelector('.shell-window[data-window-app="coding-agents"] [data-window-app-icon]')?.getAttribute('src') || null }));
    rec('M-09', 'echte App (coding-agents, v4-Manifest): Startmenü, Reiter und Fenster nutzen dieselbe Icon-URL', srcs.launcher && srcs.launcher === srcs.win && srcs.tab === srcs.win, Object.fromEntries(Object.entries(srcs).map(([k, v]) => [k, v && v.slice(0, 70)])));
    await P.screenshot({ path: shotPath('M-04', 'coding-agents-kopf-1280') });
    await P.screenshot({ path: shotPath('M-04', 'coding-agents-kopf-zoom'), clip: { x: 20, y: 0, width: 520, height: 160 } });
    rec('M-04', 'Coding Agents: Standard-Icon-Header der Shell (Icon geladen), kein eigenes Kopf-Icon, Titel rechts neben dem Icon, nichts in den Zonen', head.iconImg && !head.ownMark && head.ownIcons.length === 0 && head.title && head.title[0] >= head.icon[0] + head.icon[2] - 2 && zt.under.length === 0, { head, under: zt.under, syncText: zt.syncText });
    rec('M-03', 'Coding Agents leer (keine Läufe): kein „Daten werden synchronisiert“', !zt.syncText, { syncText: zt.syncText, longTexts: zt.longTexts.slice(0, 4) }); // v4-allow: code (Laborprüfung: App-Titel bzw. zitierter Owner-Wortlaut)
    rec('M-04', 'ohne Seitenfehler', perr.length === 0, perr.slice(0, 3));
    await ctx.close();
  }
  if (want('M13') || want('M15') || want('M10')) {
    for (const id of (process.env.MG_APPS || 'credentials,mail,outbound,news,buchhaltung').split(',')) {
      for (const w of [1280, 420]) {
        if (id !== 'credentials' && w === 420) continue;
        const { ctx, P, perr } = await real(id, w);
        const zt = await zoneAndText(P, id);
        if (w === 420) {
          const tb = await P.evaluate(() => { const t = [...document.querySelectorAll('[data-module-tabs] .module-tab')].filter((x) => x.checkVisibility()).map((x) => x.getBoundingClientRect()); const a = document.querySelector('[data-v4-topbar] .account-button')?.getBoundingClientRect(); const d = document.querySelector('[data-v4-topbar] [data-v4-sync]')?.getBoundingClientRect(); return { tabsRight: Math.round(Math.max(...t.map((r) => r.right))), syncLeft: d && Math.round(d.left), avatarLeft: a && Math.round(a.left) }; });
          rec('NEBEN', `${id} 420: Reiter überlappt Zustandspunkt/Avatar nicht`, tb.tabsRight <= Math.min(tb.syncLeft ?? 1e9, tb.avatarLeft ?? 1e9), tb);
          await P.screenshot({ path: shotPath('NEBEN', `${id}-420-kopfleiste`), clip: { x: 0, y: 0, width: 420, height: 50 } });
        }
        await P.screenshot({ path: shotPath(id === 'credentials' || id === 'buchhaltung' ? 'M-13' : 'M-15', `${id}-${w}`) });
        if (id === 'credentials') {
          // „Neue Zugangsdaten“ öffnen (echte Maus), dort die Erklärsätze zählen
          const add = await P.$$(`.shell-window[data-window-app="credentials"] .v4-app-root button`);
          let opened = null;
          for (const b of add) { const t = (await b.evaluate((e) => (e.getAttribute('aria-label') || e.textContent).trim())); if (/neu|hinzuf|anlegen/i.test(t)) { const c = await mclick(P, b, { must: false }); opened = { t, ok: c.ok }; break; } }
          await settle(P, 600);
          const zt2 = await zoneAndText(P, id);
          await P.screenshot({ path: shotPath('M-13', `credentials-neu-${w}`) });
          rec('M-13', `credentials ${w}: keine Info-Box im Inhalt, kein „Write-only“-Kasten, keine Erklärsätze neben Feldern`, zt.boxes.length === 0 && !zt.writeOnly && !zt2.writeOnly && zt2.boxes.length === 0 && zt2.longTexts.length === 0, { boxes: zt.boxes, long: zt.longTexts, opened, boxesNeu: zt2.boxes, longNeu: zt2.longTexts, writeOnly: [zt.writeOnly, zt2.writeOnly] });
          rec('M-10', `credentials ${w}: kein App-Element unter Icon/Knopfcluster`, zt.under.length === 0 && zt2.under.length === 0, { under: zt.under, under2: zt2.under });
        } else {
          rec(id === 'buchhaltung' ? 'M-13' : 'M-15', `${id} 1280: Info-Boxen und lange Erklärtexte im Inhalt (Stichprobe)`, zt.boxes.length === 0 && zt.longTexts.length <= 1, { boxes: zt.boxes, long: zt.longTexts });
        }
        await ctx.close();
      }
    }
  }
} catch (e) {
  rec('LAUF', 'Ausnahme', false, { e: String(e?.stack || e).slice(0, 1200) });
} finally {
  await browser.close();
  server.close();
  geo.close();
}
console.log(`\n${out.filter((r) => r.ok).length}/${out.length} JA`);
