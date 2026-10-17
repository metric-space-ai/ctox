// Playwright-Abnahme des Shell-Rahmens v4 gegen die Kernel-Attrappe.
// Lauf: node src/apps/business-os/v4/frame/lab/frame.test.mjs [mock|stub]
//   mock = eigene Kernel-Attrappe (lab/index.html), stub = Kern-Stub createStubKernel (lab/stub-lab.html),
//   Stub-Stand 8265bd7 (Standard hier, sonst CTOX_APP_API_STUB).
import { createRequire } from 'node:module';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { realManifestRoutes } from '../../tools/port/real-manifest.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
// Stub-Pin 8265bd7 (abweichend per CTOX_APP_API_STUB); vor dem Laden von serve.mjs, das den Pfad beim Import liest.
process.env.CTOX_APP_API_STUB ||= path.join(os.homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
const { startServer } = await import('./serve.mjs');
const { chatCss } = await import('./extract-desktop-css.mjs');
const require = createRequire(path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
const { chromium } = require('playwright');
const exe = path.join(os.homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');
const shots = path.join(here, 'shots');
fs.mkdirSync(shots, { recursive: true });

const MODE = process.argv[2] === 'stub' ? 'stub' : 'mock';
const STUB = MODE === 'stub';
const shot = (name) => path.join(shots, `integrated-${STUB ? 'stub-' : ''}${name}`);
const results = [];
function check(name, ok, detail = '') {
  results.push({ name, ok: !!ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} [${MODE}] ${name}${detail ? ` – ${detail}` : ''}`);
}

// FR6d: Das Startmenü (launcher.js) ist ein Popup aus dem Raster-Knopf; Suche, Einträge und Fuß sind nur offen bedienbar.
async function startOpen(p) {
  if (!(await p.$eval('[data-v4-launcher]', (e) => e.classList.contains('is-active')))) await p.click('[data-shell-start]');
  await p.waitForFunction(() => document.querySelector('[data-v4-launcher]')?.classList.contains('is-active'));
}

const server = await startServer(0);
const base = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ executablePath: exe, headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
page.on('pageerror', (e) => errors.push(String(e)));
page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
// Zähler für globale Listener (window/document/visualViewport) und lebende Resize-/MutationObserver.
await page.addInitScript(() => {
  const live = new Map();
  const key = (t, type, fn, opts) => `${type}|${typeof opts === 'boolean' ? opts : !!opts?.capture}`;
  const tracked = (t) => t === window || t === document || t === window.visualViewport;
  const add = EventTarget.prototype.addEventListener;
  const rem = EventTarget.prototype.removeEventListener;
  EventTarget.prototype.addEventListener = function (type, fn, opts) {
    if (tracked(this) && fn && !(opts && opts.once)) {
      const m = live.get(this) || new Map(); live.set(this, m);
      const k = key(this, type, fn, opts); const set = m.get(k) || new Set(); set.add(fn); m.set(k, set);
    }
    return add.call(this, type, fn, opts);
  };
  EventTarget.prototype.removeEventListener = function (type, fn, opts) {
    if (tracked(this)) live.get(this)?.get(key(this, type, fn, opts))?.delete(fn);
    return rem.call(this, type, fn, opts);
  };
  let observers = 0;
  for (const Name of ['ResizeObserver', 'MutationObserver']) {
    const Orig = window[Name];
    window[Name] = class extends Orig {
      observe(...a) { if (!this.__on) { this.__on = true; observers++; } return super.observe(...a); }
      disconnect() { if (this.__on) { this.__on = false; observers--; } return super.disconnect(); }
    };
  }
  window.__listenerCount = () => {
    let n = 0;
    for (const m of live.values()) for (const set of m.values()) n += set.size;
    return { listeners: n, observers };
  };
});

// ================= Z45 (ZONE45, §16.45): ehrlicher Verbindungszustand am Meldungs-Badge =================
// Eigene Seite. Z45_ONLY=1 lässt nur diesen Block laufen. stub: Pin mit metrics.collections (74137b1) bzw. ohne (68b83a3).
async function z45() {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const P = await ctx.newPage();
  const perr = [];
  P.on('pageerror', (e) => perr.push(String(e)));
  P.on('console', (m) => { if (m.type() === 'error' && !/404|Failed to load resource/.test(m.text())) perr.push(m.text()); });
  await P.addInitScript(() => {
    let observers = 0;
    for (const Name of ['ResizeObserver', 'MutationObserver']) {
      const Orig = window[Name];
      window[Name] = class extends Orig {
        observe(...a) { if (!this.__on) { this.__on = true; observers++; } return super.observe(...a); }
        disconnect() { if (this.__on) { this.__on = false; observers--; } return super.disconnect(); }
      };
    }
    window.__z45Observers = () => observers;
  });
  await P.route(/\/~a\/z45@/, (route) => route.fulfill({ status: 200, contentType: 'text/javascript', body: `export function mount(root, api) {
    root.innerHTML = '<div style="font:13px system-ui;padding:12px" data-z45>Abgleich-Probe</div>';
    // Abonniert die eigene (leere) Collection wie eine echte App (Z45-11).
    let off = null;
    try { off = api.query('z45_items', {}).subscribe(() => {}); } catch { off = null; }
    return { unmount() { try { off?.(); } catch {} root.replaceChildren(); }, snapshot() { return {}; } };
  }` }));
  await P.goto(`${base}/v4/frame/lab/${STUB ? `stub-lab.html?inst=z45-${Date.now().toString(36)}` : 'index.html'}`);
  await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  // Z45-11: Verlauf des App-Zustands ab VOR der Veröffentlichung (der Kern abonniert Manifest-Collections selbst).
  await P.evaluate(async () => {
    const st = (await import('/v4/shell/sync-state.js')).currentSyncState();
    window.__z45Hist = [];
    const rec = () => { const l = st.getAppState('z45', ['z45_items']).level; if (window.__z45Hist.at(-1) !== l) window.__z45Hist.push(l); };
    window.__z45HistOff = st.subscribe(rec, { app: true });
  });
  await P.evaluate(async (stub) => {
    const layout = { category: 'Labor', icon_svg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" fill="#2563eb"/></svg>', default_width: 420, default_height: 280 };
    if (stub) await window.__lab.kernel.sim.publishAppVersion('z45', '1.0.0', { title: 'Abgleich', layout, styles: [], collections: { z45_items: {} } });
    else window.__lab.kernel.sim.addApp({ id: 'z45', title: 'Abgleich', active_ver: '1.0.0', activated_ver: '1.0.0', state: 'activated', target_ver: null, error: null },
      { app_id: 'z45', app_ver: '1.0.0', entry: 'index.js', layout, collections: { z45_items: {} } });
  }, STUB);
  await P.waitForSelector('[data-v4-app="z45"]', { state: 'attached' });
  const open = (id, g) => P.evaluate(async ([id, g]) => { const h = window.__lab.frame.shellHandle(); const o = h.openWindow(id, { geometry: g }); await h.win(o).host.ready; return o; }, [id, g]);
  const zw = await open('z45', { x: 40, y: 40, w: 420, h: 280 });
  const nw = await open('notes', { x: 520, y: 40, w: 420, h: 280 });
  await P.waitForSelector('.shell-window[data-window-app="z45"] [data-z45]');
  // Texte nur als Schlüssel (T.warn/T.error/T.action aus SYNC_TEXT): Ergebnisdateien tragen keine Zustandstexte.
  {
    // Z45-11: App mit leerer Collection → zuerst gelb bzw. grau, nach Zustellung grün, nie dauerhaft grau (Pin 7b9560d).
    // Pins ohne Abo-Liveness (74137b1) bzw. ohne collections (68b83a3): grau, nie grün. Kein Badge-Eintrag dabei.
    await P.waitForFunction(() => window.__z45Hist.at(-1) === 'ok', null, { timeout: 3000 }).catch(() => null);
    const r = await P.evaluate(() => ({ hist: window.__z45Hist.slice(), listed: !!window.__lab.kernel.metrics.get()?.collections?.z45_items }));
    await P.evaluate(() => { window.__z45HistOff?.(); });
    const zone = await P.evaluate((o) => window.__lab.frame.shellHandle().win(o).host.zone.entries().filter((e) => e.code === 'shell.sync').length, zw);
    const ok = r.listed
      ? r.hist[0] !== 'ok' && r.hist.includes('warn') && r.hist.at(-1) === 'ok' && zone === 0
      : (STUB ? r.hist.at(-1) === 'unknown' && !r.hist.includes('ok') && zone === 0 : true);
    if (STUB || r.listed) check(`Z45-11 leere abonnierte Collection: ${r.listed ? 'gelb/grau → grün nach Zustellung, nie dauerhaft grau' : 'Pin ohne Abo-Liveness → grau, nie grün'}, kein Badge-Eintrag`, ok, JSON.stringify({ ...r, zone }));
  }
  const zoneOf = (o) => P.evaluate(async (o) => {
    const { SYNC_TEXT: T } = await import('/v4/shell/sync-state.js');
    const k = (t) => (t === T.warn ? 'T.warn' : t === T.error ? 'T.error' : t === T.action ? 'T.action' : t);
    return window.__lab.frame.shellHandle().win(o).host.zone.entries().filter((e) => e.code === 'shell.sync').map((e) => `${e.level}|${k(e.text)}|${k(e.action?.label ?? '')}`);
  }, o);
  const overall = () => P.evaluate(async () => (await import('/v4/shell/sync-state.js')).currentSyncState()?.getOverallState() ?? null);
  const badge = (o) => P.evaluate((o) => { const z = window.__lab.frame.shellHandle().win(o).handle.element.querySelector('.v4-status-zone'); return z && !z.hidden ? { level: z.dataset.level, n: z.querySelector('.v4-status-zone__badge').textContent } : null; }, o);
  const settle = (ms) => P.waitForTimeout(ms);
  // Labor-Hebel je Kernel: stub = sim.setBacklog/failCollection/setLink, mock = metrics.set mit collections.
  const setCol = (state) => P.evaluate(async ([stub, state]) => {
    const k = window.__lab.kernel;
    if (stub) {
      if (state === 'catching_up') await k.sim.setBacklog(3, 'z45_items');
      else if (state === 'error') { await k.sim.setBacklog(0, 'z45_items'); await k.sim.failCollection('z45_items', 'forbidden', 'Recht entzogen'); }
      else if (state === 'other') await k.sim.setBacklog(2, 'files');
      else { await k.sim.failCollection('z45_items', null); await k.sim.setBacklog(0, 'z45_items'); await k.sim.setBacklog(0, 'files'); }
      return;
    }
    const base = k.metrics.get();
    const col = (s) => ({ state: s, backlog: s === 'catching_up' ? 3 : 0, gen: 1, since_ms: Date.now(), ...(s === 'error' ? { error: { code: 'forbidden', message: 'Recht entzogen' } } : {}) });
    const collections = { notes: col('live'), files: col(state === 'other' ? 'catching_up' : 'live'), z45_items: col(state === 'other' ? 'live' : state) };
    k.metrics.set({ ...base, collections });
  }, [STUB, state]);

  const v4 = STUB ? await P.evaluate(() => !!window.__lab.kernel.metrics.get()?.collections) : true;
  if (!v4) {
    // Pin ohne §16.45 (68b83a3): kein Beleg → unknown, nie ok; kein Badge-Eintrag, auch bei Rückstand.
    await P.evaluate(() => window.__lab.kernel.sim.setBacklog(3));
    await settle(2400);
    const o = await overall();
    check('Z45 stub ohne metrics.collections (Pin vor K-1e): Gesamtzustand unknown (nie ok), kein Verbindungseintrag im Badge', o?.level === 'unknown' && (await zoneOf(zw)).length === 0, JSON.stringify({ o, zone: await zoneOf(zw) }));
    await P.evaluate(() => window.__lab.kernel.sim.setBacklog(0));
  } else {
    if (!STUB) await setCol('live');
    await settle(150);
    const o0 = await overall();
    const z0 = await zoneOf(zw);
    const a0 = await P.evaluate(async () => (await import('/v4/shell/sync-state.js')).currentSyncState()?.getAppState('z45'));
    check('Z45-1 alles live: Gesamtzustand ok, kein Verbindungseintrag am Badge', o0?.level === 'ok' && z0.length === 0 && (await badge(zw)) === null, JSON.stringify({ o0: o0 && { level: o0.level, source: o0.source }, app: a0 && { level: a0.level, unknown: a0.unknown, collections: a0.collections }, z0 }));
    await setCol('catching_up');
    await settle(400);
    const zEarly = await zoneOf(zw);
    const oWarn = await overall();
    await settle(2100);
    const zWarn = await zoneOf(zw);
    const bWarn = await badge(zw);
    const nWarn = await zoneOf(nw);
    await P.screenshot({ path: shot('z45-warn-1280.png') });
    const action = STUB ? 'T.action' : '';
    check('Z45-2 catching_up: Zustandspunkt sofort gelb (warn), Badge-Eintrag (T.warn, gelb) erst nach 2 s, mit Aktion T.action (stub); andere App bleibt still',
      oWarn?.level === 'warn' && oWarn.pending.includes('z45_items') && zEarly.length === 0 && JSON.stringify(zWarn) === JSON.stringify([`warn|T.warn|${action || 'Ausblenden'}`]) && bWarn?.level === 'warn' && nWarn.length === 0,
      JSON.stringify({ oWarn: oWarn && { level: oWarn.level, pending: oWarn.pending }, zEarly, zWarn, bWarn, nWarn }));
    if (STUB) {
      const before = await P.evaluate(() => { window.__z45Marker = 'kein-reload'; return window.__lab.kernel.sim.transportConnects; });
      await P.click('.shell-window[data-window-app="z45"] .v4-status-zone__badge');
      await P.click('.shell-window[data-window-app="z45"] .v4-status-zone__list .v4-status-zone__action[data-code="shell.sync"]:not([data-hide])');
      await settle(200);
      const after = await P.evaluate(() => window.__lab.kernel.sim.transportConnects);
      check('Z45-3 Aktion T.action ruft den Kernel-Neuaufbau (§16.45; sim.transportConnects +1, kein Seiten-Reload)', after === before + 1 && (await P.evaluate(() => window.__z45Marker === 'kein-reload')), JSON.stringify({ before, after }));
    }
    await setCol('live');
    await settle(300);
    const zGone = await zoneOf(zw);
    check('Z45-4 wieder alles live: Eintrag verschwindet still, Gesamtzustand ok', zGone.length === 0 && (await overall())?.level === 'ok' && (await badge(zw)) === null, JSON.stringify(zGone));
    await setCol('error');
    await settle(150);
    const zErr = await zoneOf(zw);
    const oErr = await overall();
    const bErr = await badge(zw);
    await P.screenshot({ path: shot('z45-error-1280.png') });
    check('Z45-5 error: sofort T.error am Badge (rot), Gesamtzustand error mit Code',
      JSON.stringify(zErr) === JSON.stringify([`error|T.error|${action || 'Ausblenden'}`]) && bErr?.level === 'error' && oErr?.level === 'error' && oErr.errors[0]?.code === 'forbidden',
      JSON.stringify({ zErr, bErr, oErr: oErr && { level: oErr.level, errors: oErr.errors } }));
    await setCol('live');
    // Gegenprobe (Mutationsbeleg der Zuordnung): Rückstand in einer Collection, die die App nicht angefordert hat →
    // Gesamtzustand gelb, aber kein Eintrag am Badge der App.
    await setCol('other');
    await settle(2400);
    const oOther = await overall();
    const zOther = await zoneOf(zw);
    check('Z45-6 Gegenprobe: Rückstand in fremder Collection → Gesamtzustand warn, Badge der App bleibt still', oOther?.level === 'warn' && zOther.length === 0, JSON.stringify({ o: oOther && { level: oOther.level, pending: oOther.pending }, zOther }));
    await setCol('live');
    if (!STUB) {
      // V3-Rückfall: Kernel ohne metrics.collections, Diagnose als Objekt und als Funktion.
      await P.evaluate(() => { const k = window.__lab.kernel; const { collections, ...rest } = k.metrics.get(); k.metrics.set(rest); });
      const v3 = async (asFn) => {
        await P.evaluate((asFn) => {
          const snap = { collections: { z45_items: { connectionStatus: 'connected', queryReady: true, lastLifecycleEvent: { code: 'peer_connect_timeout' } } } };
          window.ctoxBusinessOsSyncDiagnostics = asFn ? () => snap : snap;
          window.dispatchEvent(new CustomEvent('ctox-business-os-sync-diagnostics', { detail: snap }));
        }, asFn);
        await settle(2400);
        const r = { o: (await overall())?.level, z: await zoneOf(zw) };
        await P.evaluate(() => { window.ctoxBusinessOsSyncDiagnostics = { collections: { z45_items: { connectionStatus: 'connected', queryReady: true } } }; window.dispatchEvent(new CustomEvent('ctox-business-os-sync-diagnostics')); });
        await settle(300);
        r.after = { o: (await overall())?.level, z: await zoneOf(zw) };
        return r;
      };
      const vObj = await v3(false);
      const vFn = await v3(true);
      const want = JSON.stringify({ o: 'warn', z: ['warn|T.warn|Ausblenden'], after: { o: 'ok', z: [] } });
      check('Z45-7 V3-Rückfall (Objekt und Funktion) mit peer_connect_timeout → warn am Badge, connected+queryReady → ok und still', JSON.stringify(vObj) === want && JSON.stringify(vFn) === want, JSON.stringify({ vObj, vFn }));
      await P.evaluate(() => { delete window.ctoxBusinessOsSyncDiagnostics; window.dispatchEvent(new CustomEvent('ctox-business-os-sync-diagnostics')); });
      await settle(1200);
      check('Z45-8 keine Quelle (weder V4 noch V3) → unknown, nie ok', (await overall())?.level === 'unknown');
    }
  }
  // Z45-10 (Runde 2): Verbindung weg → genau EIN Eintrag je App-Badge (eine Quelle: sync-state; der frühere
  // metrics.link-Eintrag „Keine Verbindung zum Server“ ist entfallen). 5-s-Regel für reine Verbindungsaussetzer.
  {
    const allOf = (o) => P.evaluate(async (o) => {
      const { SYNC_TEXT: T } = await import('/v4/shell/sync-state.js');
      return window.__lab.frame.shellHandle().win(o).host.zone.entries().map((e) => `${e.level}|${e.code}|${e.text === T.warn ? 'T.warn' : e.text === T.error ? 'T.error' : 'anderer Text'}`);
    }, o);
    const setLink = (state) => P.evaluate(async ([stub, state]) => {
      const k = window.__lab.kernel;
      if (stub) await k.sim.setLink({ state });
      else k.metrics.set({ ...k.metrics.get(), link: { state, rtt_ms: 0, relay: false } });
    }, [STUB, state]);
    await setLink('offline');
    await settle(3000);
    const at3 = await allOf(zw);
    await settle(2600);
    const at56 = { z45: await allOf(zw), notes: await allOf(nw), overall: (await overall())?.level };
    await P.screenshot({ path: shot('z45-offline-1280.png') });
    await setLink('online');
    await settle(400);
    const after = { z45: await allOf(zw), notes: await allOf(nw) };
    const one = (list) => list.length === 1 && list[0] === 'warn|shell.sync|T.warn';
    check('Z45-10 Netz weg (link.state): nach > 5 s genau EIN Eintrag je App-Badge (shell.sync, T.warn; kein metrics.link), bei 3 s noch still, wieder online → still',
      at3.length === 0 && one(at56.z45) && one(at56.notes) && at56.overall === 'warn' && after.z45.length === 0 && after.notes.length === 0,
      JSON.stringify({ at3, at56, after }));
  }
  // Leck je Shell-Tausch (FR6a-Befund): Observer vor dem ersten und nach jedem Tausch gleich.
  const obs = [await P.evaluate(() => window.__z45Observers())];
  for (let i = 0; i < 3; i++) { await P.evaluate(() => window.__lab.swapShell()); await settle(150); obs.push(await P.evaluate(() => window.__z45Observers())); }
  const syncAlive = await P.evaluate(async () => !!(await import('/v4/shell/sync-state.js')).currentSyncState());
  check('Z45-9 Shell-Tausch ohne Leck: Observer-Zahl ab dem ersten Tausch konstant, genau ein Sync-Zustand aktiv', obs.every((n) => n === obs[0]) && syncAlive, obs.join(' → '));
  check('Z45 ohne Seitenfehler', perr.length === 0, perr.join(' | '));
  await ctx.close();
}

// ================= FR6b: Taskleiste je App, Desktop anzeigen, Meldeschleife (Shell-Seite) =================
// Eigene Seite. FR6B_ONLY=1 lässt nur diesen Block laufen (schnelle Mutationsläufe).
async function fr6b() {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const P = await ctx.newPage();
  const perr = [];
  P.on('pageerror', (e) => perr.push(String(e)));
  P.on('console', (m) => { if (m.type() === 'error' && !/404|Failed to load resource/.test(m.text())) perr.push(m.text()); });
  // Crew-Chats: drei Crew-Apps (presentation.taskbar_group = 'crew'), Code kommt über /~a/ (hier vom Labor geliefert).
  const CREW = [['crew-anna', 'Anna', '#c2410c'], ['crew-ben', 'Ben', '#0f766e'], ['crew-cleo', 'Cleo', '#7c3aed']];
  await P.route(/\/~a\/crew-[a-z]+@/, (route) => {
    const id = /\/~a\/(crew-[a-z]+)@/.exec(route.request().url())[1];
    const name = CREW.find((c) => c[0] === id)?.[1] || id;
    route.fulfill({ status: 200, contentType: 'text/javascript', body: `export function mount(root, api) {
      root.innerHTML = '<div style="font:13px system-ui;padding:12px 14px"><p>Crew-Chat</p><input data-lab-input placeholder="Nachricht" style="width:90%;height:28px"></div>';
      api.shell?.setTitle?.('Chat ${name}');
      return { unmount() { root.replaceChildren(); }, snapshot() { return {}; } };
    }` });
  });
  await P.goto(`${base}/v4/frame/lab/${STUB ? `stub-lab.html?inst=fr6b-${Date.now().toString(36)}` : 'index.html'}`);
  await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  await P.evaluate(async ([stub, crew]) => {
    const add = (id, title, layout, ver = '1.0.0') => (stub
      ? window.__lab.kernel.sim.publishAppVersion(id, ver, { title, layout, styles: [] })
      : window.__lab.kernel.sim.addApp({ id, title, active_ver: ver, activated_ver: ver, state: 'activated', target_ver: null, error: null },
        { app_id: id, app_ver: ver, entry: 'index.js', layout }));
    for (const [id, name, color] of crew) {
      const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" fill="${color}"/><circle cx="32" cy="26" r="11" fill="#fff"/><path d="M12 58c3-13 12-19 20-19s17 6 20 19" fill="#fff"/></svg>`;
      await add(id, `Crew ${name}`, { category: 'Crew', icon_svg: svg, default_width: 420, default_height: 320, presentation: { taskbar_group: 'crew' } });
    }
    const crmSvg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" fill="#7b5cc4"/><text x="32" y="42" font-family="system-ui,sans-serif" font-size="28" font-weight="600" fill="#fff" text-anchor="middle">K</text></svg>';
    await add('crm', 'Kontakte', { category: 'Vertrieb', icon_svg: crmSvg, default_width: 520, default_height: 340 }, '2.0.0');
  }, [STUB, CREW]);
  await P.waitForSelector('[data-v4-app="crew-cleo"]', { state: 'attached' });
  await P.waitForSelector('[data-v4-app="crm"]', { state: 'attached' });
  const open = (id, g) => P.evaluate(async ([id, g]) => { const h = window.__lab.frame.shellHandle(); const o = h.openWindow(id, { geometry: g }); await h.win(o).host.ready; return o; }, [id, g]);
  const ids = {};
  ids.notes = await open('notes', { x: 30, y: 30, w: 520, h: 340 });
  ids.tasks = await open('tasks', { x: 580, y: 30, w: 520, h: 340 });
  ids.ledger = await open('ledger', { x: 300, y: 300, w: 520, h: 340 });
  const crewWins = [];
  for (let i = 0; i < 6; i++) crewWins.push(await open(CREW[i % 3][0], { x: 60 + i * 60, y: 120 + i * 40, w: 420, h: 300 }));
  await P.waitForTimeout(300);
  // FR6c: die einzige Leiste ist die Reiterleiste oben (.module-tabs); Einträge = Reiter, auch die im Überlauf „+N“.
  const bar = () => P.evaluate(() => {
    const el = document.querySelector('[data-module-tabs]');
    const top = document.querySelector('[data-v4-topbar] .topbar');
    const tr = el.getBoundingClientRect();
    const all = [...el.querySelectorAll('.module-tab[data-v4-task-group]')];
    const vis = all.filter((n) => !n.closest('details'));
    const more = el.querySelector(':scope > details.module-overflow');
    return {
      entries: all.map((n) => ({ group: n.dataset.v4TaskGroup, label: n.querySelector('.module-tab-label').textContent, count: n.querySelector('[data-v4-count]').hidden ? 0 : Number(n.querySelector('[data-v4-count]').textContent), countShown: getComputedStyle(n.querySelector('[data-v4-count]')).display !== 'none', title: n.title, active: n.getAttribute('aria-current') === 'page', hidden: !!n.closest('details') })),
      more: more ? more.querySelector('summary').textContent : '',
      fits: vis.every((n) => { const r = n.getBoundingClientRect(); return r.left >= tr.left - 0.5 && r.right <= tr.right + 0.5; }) && (!more || more.getBoundingClientRect().right <= tr.right + 0.5)
        && top.scrollWidth <= top.clientWidth + 1 && document.documentElement.scrollWidth <= innerWidth,
      bottomBar: !!document.querySelector('[data-v4-taskbar], .v4f-bar'),
      barW: Math.round(tr.width),
    };
  });
  const b1 = await bar();
  const crewEntry = b1.entries.find((e) => e.group === 'crew');
  check('FR6b-T1 (FR6c) Reiterleiste oben: EIN Reiter je App, 6 Crew-Chats unter EINEM Reiter „Crew“ mit Zahl 6 (sichtbar); Einzelfenster ohne Zahl; keine Taskleiste unten',
    !b1.bottomBar && b1.entries.length === 4 && crewEntry?.label === 'Crew' && crewEntry?.count === 6 && crewEntry?.countShown && b1.entries.filter((e) => e.group !== 'crew').every((e) => e.count === 0 && !e.countShown)
      && JSON.stringify(b1.entries.map((e) => e.group)) === JSON.stringify(['notes', 'tasks', 'ledger', 'crew']), JSON.stringify(b1));
  await P.screenshot({ path: shot('fr6b-taskbar-1280.png') });

  // Klick wechselt reihum durch die Fenster der Gruppe.
  const focusedOwner = () => P.evaluate(() => window.__lab.frame.shellHandle().windows().find((r) => r.handle.element.classList.contains('is-focused') && r.handle.element.style.display !== 'none')?.ownerId || '');
  await P.click('[data-v4-task-group="ledger"]'); // Ausgangslage: nicht-Crew-Fenster vorn
  const seen = [];
  for (let i = 0; i < 7; i++) { await P.click('[data-v4-task-group="crew"]'); await P.waitForTimeout(60); seen.push(await focusedOwner()); }
  const distinct = new Set(seen.slice(0, 6));
  check('FR6b-T2 Klick auf „Crew“ wechselt reihum: 6 Klicks = 6 verschiedene Crew-Fenster, der 7. beginnt von vorn',
    distinct.size === 6 && [...distinct].every((o) => crewWins.includes(o)) && seen[6] === seen[0], JSON.stringify(seen));

  // Rechtsklick: Fensterliste der Gruppe; Eintrag wählen holt genau dieses Fenster; Esc schließt.
  await P.click('[data-v4-task-group="crew"]', { button: 'right' });
  const menu = await P.evaluate(() => { const m = document.querySelector('[data-v4-task-menu]'); const r = m.getBoundingClientRect(); const t = document.querySelector('[data-v4-task-group="crew"]').getBoundingClientRect(); return { open: !m.hidden, items: [...m.children].map((i) => i.textContent.trim()), inBar: !!m.closest('[data-v4-topbar]') && r.top >= t.bottom - 1 && r.top <= t.bottom + 12 }; });
  await P.screenshot({ path: shot('fr6b-taskbar-menu-1280.png') });
  const pickId = await P.$eval('[data-v4-task-menu] [data-v4-task-menu-win]:nth-child(3)', (b) => b.dataset.v4TaskMenuWin).catch(() => '');
  if (pickId) await P.click('[data-v4-task-menu] [data-v4-task-menu-win]:nth-child(3)');
  await P.waitForTimeout(60);
  const picked = await P.evaluate((id) => document.getElementById(id)?.classList.contains('is-focused') || [...document.querySelectorAll('.shell-window.is-focused')].some((w) => w.dataset.windowId && w.id === id), pickId);
  const pickedOwner = await focusedOwner();
  const menuClosed = await P.$eval('[data-v4-task-menu]', (m) => m.hidden);
  await P.click('[data-v4-task-group="crew"]', { button: 'right' });
  await P.keyboard.press('Escape');
  const escClosed = await P.$eval('[data-v4-task-menu]', (m) => m.hidden);
  check('FR6b-T3 (FR6c) Rechtsklick auf den Reiter zeigt die Fensterliste (6 Chats, direkt unter dem Reiter); Auswahl holt genau dieses Fenster nach vorn; Esc schließt',
    menu.open && menu.items.length === 6 && menu.inBar && menu.items.every((t) => /^Chat (Anna|Ben|Cleo)$/.test(t)) && menuClosed && escClosed && crewWins.includes(pickedOwner)
      && (await P.evaluate(([o, id]) => window.__lab.frame.shellHandle().win(o)?.id === id, [pickedOwner, pickId])), JSON.stringify({ menu, pickId, picked, pickedOwner, menuClosed, escClosed }));

  // Leiste wächst nie über ihre Breite: wird sie schmal, wandern Reiter in „+N“ (Shell-V2), der aktive bleibt sichtbar.
  const fitsWide = b1.fits && !b1.more;
  await P.click('[data-v4-task-group="tasks"]'); // tasks = aktiv (nicht der letzte Reiter)
  await P.waitForTimeout(80);
  await P.evaluate(() => { document.querySelector('[data-module-tabs]').style.maxWidth = '260px'; });
  await P.waitForTimeout(250);
  const bNarrow = await bar();
  await P.evaluate(() => { document.querySelector('[data-module-tabs]').style.maxWidth = ''; });
  await P.waitForTimeout(250);
  const bBack = await bar();
  const activeN = bNarrow.entries.find((e) => e.active);
  check('FR6b-T4 (FR6c) Reiterleiste wächst nie über ihre Breite: 1280 alles sichtbar ohne „+N“; auf 260 px Überlauf „+N“ (gruppiert, Crew zählt als EIN Eintrag), der aktive Reiter bleibt sichtbar, alles passt; wieder breit → kein „+N“',
    fitsWide && /^\+\d+$/.test(bNarrow.more) && Number(bNarrow.more.slice(1)) === bNarrow.entries.filter((e) => e.hidden).length && bNarrow.entries.length === 4
      && activeN?.group === 'tasks' && !activeN.hidden && bNarrow.fits && !bBack.more && bBack.fits, JSON.stringify({ b1: { more: b1.more, fits: b1.fits }, bNarrow, bBack: { more: bBack.more, fits: bBack.fits } }));

  // 420 px (Telefonbreite): Leiste passt, Gruppen bleiben.
  await P.setViewportSize({ width: 420, height: 800 });
  await P.waitForTimeout(300);
  const b420 = await bar();
  await P.screenshot({ path: shot('fr6b-taskbar-420.png') });
  const gear420 = await P.$eval('[data-v4-settings-button]', (b) => getComputedStyle(b).display);
  check('FR6b-T5 (FR6c) 420 px: 4 Einträge, Crew mit 6, Leiste passt (keine Überbreite), der aktive Reiter ist sichtbar, Zahnrad ausgeblendet',
    b420.entries.length === 4 && b420.entries.find((e) => e.group === 'crew')?.count === 6 && b420.fits && b420.entries.some((e) => e.active && !e.hidden) && gear420 === 'none', JSON.stringify({ b420, gear420 }));
  await P.setViewportSize({ width: 1280, height: 800 });
  await P.waitForTimeout(200);

  // ---- Runde 2: Pin und Fenster EIN Eintrag (Windows) ----
  const entryInfo = () => P.evaluate(() => [...document.querySelectorAll('[data-module-tabs] .module-tab[data-v4-task-group]')].map((n) => ({
    key: n.dataset.v4TaskGroup, pin: n.dataset.v4Pin || '', count: Number(n.dataset.v4Count), pinned: n.dataset.pinned === 'true',
    label: n.querySelector('.module-tab-label').textContent, running: !!n.dataset.running,
  })));
  for (const id of ['crm', 'notes', 'crew-anna']) await P.$eval(`[data-v4-app="${id}"] [data-v4-pin]`, (b) => b.click()); // Kachel liegt unter Fenstern
  await P.waitForTimeout(200);
  const e1 = await entryInfo();
  await P.screenshot({ path: shot('fr6b-taskbar-pins-1280.png') });
  const crmIdle = e1.find((e) => e.key === 'crm');
  const notesE = e1.filter((e) => e.key === 'notes');
  const crewE = e1.filter((e) => e.key === 'crew');
  if (await P.$('[data-v4-task-group="crm"]')) await P.click('[data-v4-task-group="crm"]');
  else await P.evaluate(() => window.__lab.frame.shellHandle().openWindow('crm'));
  await P.waitForSelector('.shell-window[data-window-app="crm"]');
  await P.waitForTimeout(250);
  const e2 = await entryInfo();
  const crmRun = e2.find((e) => e.key === 'crm');
  check('FR6b-P1 (FR6c) Pin und Fenster = EIN Reiter: angeheftet ohne Fenster = Reiter mit data-pinned, ohne Laufend-Zustand, vorn in Pin-Reihenfolge; angeheftete App mit Fenster und angeheftetes Crew-Mitglied verschmelzen mit ihrem Reiter; Klick öffnet, derselbe Reiter läuft',
    JSON.stringify(e1.slice(0, 3).map((e) => e.key)) === JSON.stringify(['crm', 'notes', 'crew']) && e1.length === 5 && crmIdle && crmIdle.pinned && !crmIdle.running && crmIdle.pin === 'crm' && crmIdle.label === 'Kontakte'
      && notesE.length === 1 && notesE[0].pin === 'notes' && notesE[0].running && crewE.length === 1 && crewE[0].pin === 'crew-anna' && crewE[0].count === 6 && crewE[0].label === 'Crew'
      && e2.length === 5 && crmRun.count === 1 && crmRun.running && crmRun.pin === 'crm', JSON.stringify({ e1, e2 }));
  await P.setViewportSize({ width: 420, height: 800 });
  await P.waitForTimeout(300);
  await P.screenshot({ path: shot('fr6b-taskbar-pins-420.png') });
  await P.setViewportSize({ width: 1280, height: 800 });
  await P.waitForTimeout(200);

  // ---- FR6c: Überlauf „+N“ wie Shell-V2, gruppiert; Auswahl im Überlauf macht den Reiter aktiv und damit sichtbar ----
  if (await P.$eval('[data-v4-task-group="crm"]', (t) => t.getAttribute('aria-current') !== 'page')) { await P.click('[data-v4-task-group="crm"]'); await P.waitForTimeout(80); }
  await P.evaluate(() => { document.querySelector('[data-module-tabs]').style.maxWidth = '230px'; });
  await P.waitForTimeout(250);
  const o1 = await bar();
  await P.screenshot({ path: shot('fr6c-tabs-overflow-1280.png'), clip: { x: 0, y: 0, width: 640, height: 60 } });
  await P.click('[data-module-tabs] details.module-overflow > summary');
  await P.waitForTimeout(100);
  const list = await P.$$eval('[data-module-tabs] .module-overflow-menu .module-tab', (els) => els.map((e) => e.dataset.v4TaskGroup));
  await P.screenshot({ path: shot('fr6c-tabs-overflow-menu-1280.png') });
  const lastHidden = o1.entries.filter((e) => e.hidden).map((e) => e.group).pop();
  if (list.length) await P.click(`[data-module-tabs] .module-overflow-menu [data-v4-task-group="${lastHidden}"]`);
  await P.waitForTimeout(250);
  const o1b = await bar();
  const picked1 = o1b.entries.find((e) => e.group === lastHidden);
  await P.evaluate(() => { document.querySelector('[data-module-tabs]').style.maxWidth = ''; });
  await P.waitForTimeout(250);
  const o2 = await bar();
  check('FR6b-O1 (FR6c) Überlauf „+N“: zu schmal → hintere Reiter im Menü, der aktive (crm) bleibt sichtbar, nichts ragt über die Leiste; „+N“ listet genau die verborgenen; Auswahl aktiviert den Reiter und holt ihn aus dem Überlauf; wieder breit → kein „+N“',
    /^\+\d+$/.test(o1.more) && o1.entries.some((e) => e.hidden) && o1.fits && o1.entries.find((e) => e.group === 'crm')?.active && !o1.entries.find((e) => e.group === 'crm')?.hidden
      && JSON.stringify(list) === JSON.stringify(o1.entries.filter((e) => e.hidden).map((e) => e.group)) && picked1?.active && !picked1?.hidden && o1b.fits && !o2.more,
    JSON.stringify({ o1, list, lastHidden, picked1, o1b: { more: o1b.more, fits: o1b.fits }, o2: { more: o2.more } }));
  // Pins wieder lösen (Ausgangslage für die folgenden Fälle), crm-Fenster bleibt
  for (const id of ['crm', 'notes', 'crew-anna']) await P.$eval(`[data-v4-app="${id}"] [data-v4-pin]`, (b) => b.click()); // Kachel liegt unter Fenstern
  await P.waitForTimeout(150);

  // ---- M-18 Desktop: minimiert ALLE, zweiter Klick stellt dieselben wieder her (Stapelreihenfolge) ----
  const winState = () => P.evaluate(() => window.__lab.frame.shellHandle().windows().map((r) => ({ o: r.ownerId, vis: r.handle.element.style.display !== 'none' && !r.handle.element.classList.contains('is-minimized'), f: r.handle.element.classList.contains('is-focused'), z: Number(r.handle.element.style.zIndex) || 0 })));
  await P.click('[data-v4-task-group="crew"]', { button: 'right' });
  if (await P.$('[data-v4-task-menu] [data-v4-task-menu-win]')) await P.click('[data-v4-task-menu] [data-v4-task-menu-win]:nth-child(1)');
  else await P.click(`[data-v4-task-group="crew"]`);
  await P.click('[data-v4-task-group="ledger"]'); // ledger nach vorn
  await P.waitForTimeout(100);
  // ein Fenster bleibt vorher minimiert: es darf NICHT mit zurückkommen
  const before = await winState();
  const tasksWin = await P.evaluate((o) => window.__lab.frame.shellHandle().win(o).id, ids.tasks);
  await P.click('[data-v4-task-group="tasks"]'); await P.waitForTimeout(60);
  await P.click('[data-v4-task-group="tasks"]'); await P.waitForTimeout(400); // fokussiert → zweiter Klick minimiert
  const pre = await winState();
  const visBefore = pre.filter((w) => w.vis).sort((a, b) => a.z - b.z).map((w) => w.o);
  const topBefore = pre.filter((w) => w.vis).sort((a, b) => b.z - a.z)[0]?.o;
  await startOpen(P);
  const deskBtnFree = await P.evaluate(() => { const b = document.querySelector('[data-v4-launcher-desktop]'); const r = b.getBoundingClientRect(); const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2); return { hit: !!hit && b.contains(hit), foot: !!b.closest('[data-v4-launcher] [data-v4-launcher-foot]'), first: b.parentElement.firstElementChild === b, nextIsSettings: b.nextElementSibling?.matches('[data-v4-launcher-settings]') }; });
  await P.click('[data-v4-launcher-desktop]');
  await P.waitForTimeout(600);
  const mid = await winState();
  await P.screenshot({ path: shot('fr6b-desktop-1280.png') });
  await startOpen(P);
  await P.click('[data-v4-launcher-desktop]');
  await P.waitForTimeout(600);
  const post = await winState();
  const visAfter = post.filter((w) => w.vis).sort((a, b) => a.z - b.z).map((w) => w.o);
  const topAfter = post.filter((w) => w.vis).sort((a, b) => b.z - a.z)[0]?.o;
  check('FR6b-D1 (FR6c) „Desktop“ im Launcher-Fuß (links, daneben „Einstellungen“, frei klickbar): alle Fenster minimiert (Desktop frei); zweiter Klick stellt genau die vorher sichtbaren wieder her (gleiche Reihenfolge, gleiches vorderstes), das vorher minimierte bleibt unten',
    deskBtnFree.hit && deskBtnFree.foot && deskBtnFree.first && deskBtnFree.nextIsSettings && visBefore.length >= 8 && mid.every((w) => !w.vis) && JSON.stringify(visAfter) === JSON.stringify(visBefore) && topAfter === topBefore && !post.find((w) => w.o === ids.tasks)?.vis,
    JSON.stringify({ deskBtnFree, n: visBefore.length, midVis: mid.filter((w) => w.vis).length, same: JSON.stringify(visAfter) === JSON.stringify(visBefore), topBefore, topAfter, tasksWin, before: before.length }));

  // Tastenkürzel Strg+Alt+D: gleiche Wirkung; andere D-/B-Kürzel des Browsers bleiben unberührt.
  await P.click('[data-v4-task-group="notes"]');
  await P.waitForTimeout(100);
  await P.keyboard.press('Control+Alt+KeyD'); await P.waitForTimeout(500);
  const k1 = await winState();
  await P.setViewportSize({ width: 420, height: 800 }); await P.waitForTimeout(250);
  await P.screenshot({ path: shot('fr6b-desktop-420.png') });
  await P.setViewportSize({ width: 1280, height: 800 }); await P.waitForTimeout(250);
  await P.keyboard.press('Control+Alt+KeyD'); await P.waitForTimeout(500);
  const k2 = await winState();
  check('FR6b-D2 Strg+Alt+D: minimiert alle, erneut: stellt sie wieder her', k1.every((w) => !w.vis) && k2.filter((w) => w.vis).length === visBefore.length, JSON.stringify({ k1: k1.filter((w) => w.vis).length, k2: k2.filter((w) => w.vis).length }));
  const probe = await P.evaluate(async () => {
    const out = {};
    const keys = {
      'Strg+D (Lesezeichen)': { code: 'KeyD', key: 'd', ctrlKey: true },
      'Cmd+D (Lesezeichen macOS)': { code: 'KeyD', key: 'd', metaKey: true },
      'Alt+D (Adressleiste Windows/Linux)': { code: 'KeyD', key: 'd', altKey: true },
      'Strg+Umschalt+D (alle Tabs merken)': { code: 'KeyD', key: 'D', ctrlKey: true, shiftKey: true },
      'Cmd+Alt+D (Dock macOS)': { code: 'KeyD', key: '∂', metaKey: true, altKey: true },
      'Cmd+Umschalt+B (Lesezeichenleiste macOS)': { code: 'KeyB', key: 'b', metaKey: true, shiftKey: true },
      'Strg+B': { code: 'KeyB', key: 'b', ctrlKey: true },
      'Strg+Umschalt+B (Lesezeichenleiste Win/Linux)': { code: 'KeyB', key: 'B', ctrlKey: true, shiftKey: true },
      'Strg+Alt+D (Desktop)': { code: 'KeyD', key: 'd', ctrlKey: true, altKey: true },
    };
    for (const [name, init] of Object.entries(keys)) {
      const ev = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });
      document.body.dispatchEvent(ev);
      out[name] = ev.defaultPrevented;
      await new Promise((r) => setTimeout(r, 30));
    }
    return out;
  });
  // Rückstellung nach der Probe (Strg+Alt+D hat erneut umgeschaltet)
  await P.keyboard.press('Control+Alt+KeyD'); await P.waitForTimeout(400);
  const others = Object.entries(probe).filter(([k]) => !k.startsWith('Strg+Alt+D'));
  check('FR6b-D3 Kürzel überschreibt keinen Browser-Standard: nur Strg+Alt+D wird abgefangen; Strg+D, Cmd+D, Alt+D, Strg+Umschalt+D, Cmd+Alt+D, Cmd+Umschalt+B, Strg+B, Strg+Umschalt+B laufen unberührt durch',
    probe['Strg+Alt+D (Desktop)'] === true && others.every(([, v]) => v === false), JSON.stringify(probe));

  // ---- M-06 Meldeschleife: Blatt im App-Fenster, zwei Felder, gespeichert wie „Bugs & Features“ ----
  await P.evaluate(() => { for (const r of window.__lab.frame.shellHandle().windows()) if (r.appId !== 'notes') window.__lab.frame.shellHandle().closeWindow(r.ownerId); });
  await P.waitForTimeout(300);
  await P.evaluate((o) => { const r = window.__lab.frame.shellHandle().win(o); r.host.zone.report({ level: 'error', code: 'lab.sync', text: 'Synchronisation hängt', source: 'app' }); r.handle.container.querySelector('tr')?.setAttribute('data-record-id', 'notes-1'); r.handle.container.querySelector('tr')?.setAttribute('aria-selected', 'true'); }, ids.notes);
  await P.click('[data-v4-task-group="notes"]').catch(() => {});
  await P.waitForTimeout(100);
  if (!(await P.evaluate((o) => window.__lab.frame.shellHandle().win(o).handle.element.classList.contains('is-focused'), ids.notes))) { await P.click('[data-v4-task-group="notes"]'); await P.waitForTimeout(100); }
  const NW = '.shell-window[data-window-app="notes"]';
  // Reste früherer Proben (z. B. ein Blatt aus der Kürzel-Probe) schließen; vorher darf kein Blatt offen sein.
  for (let k = 0; k < 3 && (await P.$('[data-v4-sheet]')); k++) { await P.keyboard.press('Escape'); await P.waitForTimeout(80); }
  const preSheet = !!(await P.$('[data-v4-sheet]'));
  await P.keyboard.press('Control+Alt+KeyB');
  await P.waitForSelector(`${NW} [data-v4-sheet]`, { timeout: 3000 }).catch(() => {});
  const s1 = await P.evaluate((sel) => { const s = document.querySelector(`${sel} [data-v4-sheet]`); return s ? { title: s.querySelector('.v4-sheet-title')?.textContent, text: s.querySelector('.v4-sheet-text')?.textContent, multi: s.querySelector('[data-v4-sheet-input]')?.tagName, inWin: !!s.closest('.shell-window[data-window-app="notes"]'), outside: document.querySelectorAll('body > [data-v4-sheet], body > .v4-sheet-overlay').length } : null; }, NW);
  await P.screenshot({ path: shot('fr6b-report-sheet-1280.png') });
  await P.setViewportSize({ width: 420, height: 800 }); await P.waitForTimeout(300);
  await P.screenshot({ path: shot('fr6b-report-sheet-420.png') });
  await P.setViewportSize({ width: 1280, height: 800 }); await P.waitForTimeout(300);
  if (s1) {
    await P.fill(`${NW} [data-v4-sheet-input]`, 'Speichern ohne Wirkung');
    await P.click(`${NW} [data-v4-sheet-ok]`);
  }
  await P.waitForTimeout(150);
  const s2 = await P.evaluate((sel) => document.querySelector(`${sel} [data-v4-sheet] .v4-sheet-text`)?.textContent || '', NW);
  await P.keyboard.press('Escape');
  await P.waitForTimeout(150);
  const afterCancel = await P.evaluate((sel) => !document.querySelector(`${sel} [data-v4-sheet]`), NW);
  check('FR6b-R1 Strg+Alt+B im App-Fenster: Blatt IM Fenster „Problem melden“ – „Was ist passiert?“ (mehrzeilig), dann „Was hast du erwartet?“; Esc bricht ohne Speichern ab',
    !preSheet && s1?.title === 'Problem melden' && s1?.text === 'Was ist passiert?' && s1?.multi === 'TEXTAREA' && s1?.inWin && s1?.outside === 0 && s2 === 'Was hast du erwartet?' && afterCancel, JSON.stringify({ s1, s2, afterCancel }));

  // Programmatisch (wie FR6a aus dem App-Menü): reportProblem({appId, kind:'wish'}).
  await P.evaluate(() => { window.__fr6bReport = window.__lab.frame.reportProblem({ appId: 'notes', kind: 'wish' }); });
  await P.waitForSelector(`${NW} [data-v4-sheet]`);
  const wishTitle = await P.$eval(`${NW} .v4-sheet-title`, (e) => e.textContent);
  await P.fill(`${NW} [data-v4-sheet-input]`, 'Spalte „Fällig“ sortierbar');
  await P.click(`${NW} [data-v4-sheet-ok]`);
  await P.waitForTimeout(120);
  await P.fill(`${NW} [data-v4-sheet-input]`, 'Klick auf den Spaltenkopf sortiert');
  await P.screenshot({ path: shot('fr6b-report-sheet2-1280.png') });
  await P.click(`${NW} [data-v4-sheet-ok]`);
  const res = await P.evaluate(async () => { const r = await window.__fr6bReport; return r ? { id: r.report_id, report: r.report, bug: r.bug } : null; });
  const zoneAfter = await P.evaluate((o) => window.__lab.frame.shellHandle().win(o).host.zone.entries().map((e) => `${e.level}|${e.code}`), ids.notes);
  if (STUB) {
    const stored = await P.evaluate(async (id) => {
      const api = window.__lab.kernel.appApi('reports', '1.0.0');
      const one = (c) => new Promise((resolve) => { const lq = api.get(c, id); const v = lq.get(); if (v) { resolve(v); return; } const off = lq.subscribe((x) => { if (x) { off?.(); resolve(x); } }); setTimeout(() => resolve(null), 1500); });
      return { report: await one('business_module_reports'), bug: await one('ctox_bug_reports') };
    }, res?.id || '');
    const r = stored.report || {};
    const cc = r.client_context || {};
    check('FR6b-R2 Meldung gespeichert wie „Bugs & Features“: business_module_reports + ctox_bug_reports, kind feature, module_id notes, Felder summary/expected, angehängt App/Version/Datensatz/Zonenfehler; kein Fensterbild',
      wishTitle === 'Wunsch äußern' && r.module_id === 'notes' && r.kind === 'feature' && r.summary === 'Spalte „Fällig“ sortierbar' && r.expected === 'Klick auf den Spaltenkopf sortiert'
        && r.status === 'open' && cc.app_id === 'notes' && cc.app_ver === '1.0.0' && cc.record?.id === 'notes-1' && cc.recent_errors?.some((e) => e.code === 'lab.sync') && cc.window_image === null
        && stored.bug?.module === 'notes' && stored.bug?.payload?.kind === 'feature' && !zoneAfter.some((z) => z.includes('shell.report')),
      JSON.stringify({ wishTitle, report: r, bug: stored.bug && { module: stored.bug.module, kind: stored.bug.payload?.kind }, zoneAfter }));
  } else {
    check('FR6b-R2 (Attrappe ohne App-API-Datenteil): Speichern scheitert sichtbar NUR in der Zone des Fensters (shell.report, error), kein Rückgabewert',
      wishTitle === 'Wunsch äußern' && res === null && zoneAfter.includes('error|shell.report'), JSON.stringify({ wishTitle, res, zoneAfter }));
  }
  // ---- Runde 2: „Problem melden“ / „Wunsch äußern“ im App-Menü am Icon-Header, für ALLE Nutzer ----
  const menuIds = async () => {
    await P.keyboard.press('Escape');
    if (!(await P.$eval(NW, (w) => w.classList.contains('is-focused')))) { await P.click('[data-v4-task-group="notes"]'); await P.waitForTimeout(80); }
    await P.click(`${NW} .shell-window-v2-icon`, { position: { x: 30, y: 30 } });
    await P.waitForTimeout(80);
    return P.$$eval(`${NW} [data-v4-appmenu]`, (els) => els.map((e) => e.dataset.v4Appmenu));
  };
  const withPerms = await menuIds();
  let noPerms = withPerms;
  let perms0 = null;
  if (STUB) {
    perms0 = await P.evaluate((o) => window.__lab.frame.shellHandle().win(o).host.instance.api.actor.permissions.get(), ids.notes);
    await P.evaluate(() => window.__lab.kernel.sim.setPermissions([]));
    await P.waitForTimeout(200);
    noPerms = await menuIds();
  }
  await P.screenshot({ path: shot('fr6b-appmenu-1280.png') });
  await P.click(`${NW} [data-v4-appmenu="report-wish"]`).catch(() => {});
  await P.waitForSelector(`${NW} [data-v4-sheet]`, { timeout: 3000 }).catch(() => {});
  const viaMenu = await P.evaluate((sel) => document.querySelector(`${sel} [data-v4-sheet] .v4-sheet-title`)?.textContent || '', NW);
  await P.keyboard.press('Escape');
  await P.waitForTimeout(100);
  if (STUB) await P.evaluate((p) => window.__lab.kernel.sim.setPermissions(p), perms0);
  const rep = (l) => l.includes('report-bug') && l.includes('report-wish');
  check('FR6b-R3 App-Menü am Icon-Header: „Problem melden“ und „Wunsch äußern“ für alle Nutzer (auch ohne apps.source.view), „Code ansehen“ nur mit Recht; Eintrag öffnet das Melde-Blatt im Fenster',
    rep(withPerms) && rep(noPerms) && !noPerms.includes('code') && (STUB ? withPerms.includes('code') : !withPerms.includes('code')) && viaMenu === 'Wunsch äußern',
    JSON.stringify({ withPerms, noPerms, viaMenu }));
  check('FR6b ohne Seitenfehler', perr.length === 0, perr.join(' | '));
  await ctx.close();
}

// ================= AG (Strang AGENT, §16.44): Coding Agent als Seitenleiste der Code-Ansicht =================
// Eigene Seite, nur gegen den Kern-Stub (braucht kernel.source). Ab Stub-Pin 51b1e33 bildet der Stub §16.44 ab
// (ctox.coding.turn/cancel mit exakten Ablehnungen, coding_turns, CAS-Rückschreiben, Patch files/coding-patch:<turn_id>,
// draft.info().rev); die Shell spricht ihn über ihre App-API (v4/source/coding-port.js). Ältere Pins: nur AG-0.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/64acc2c AG_ONLY=1 node …/frame.test.mjs stub
// AG_MUTATE=<name> liefert eine mutierte Datei aus (Mutationsbeleg: der zugehörige Fall muss rot werden).
const AG_MUTATIONS = {
  enter: ['agent-panel.js', "if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); send(); }", ''],
  turnrunning: ['agent-panel.js', "text: 'Der Agent arbeitet noch an diesem Entwurf. Warte das Ergebnis ab oder brich den Lauf ab.'", "text: ''"],
  stale: ['agent-panel.js', "if (next?.rev && next.rev !== ref.rev) { ref = next; r = await dispatch('ctox.coding.turn', payload(ref), { revSource: 'retry' }, cmdOpts); }", ''],
  patch: ['agent-panel.js', 'if (txt === null) await source.draft.remove(p); else await source.draft.write(p, txt); }\n      st.applied', '}\n      st.applied'],
  cancel: ['agent-panel.js', "const res = await dispatch('ctox.coding.cancel', { turn_id: r.id });", "const res = { status: 'completed' };"],
  writerev: ['agent-panel.js', "if (ref && !ref.unsupported && confirmed?.rev) { ref = { ...ref, rev: confirmed.rev }; revSource = 'write'; }", ''],
  context: ['text-diff.js', '      if (!matches(at)) {', '      if (false) {'],
  entry: ['index.js', "shellAppApi: (() => { try { return kernel.appApi('shell', shellVer) ?? null; } catch { return null; } })() } : null;", 'shellAppApi: null } : null;'],
};
async function ag() {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const P = await ctx.newPage();
  const perr = [];
  P.on('pageerror', (e) => perr.push(String(e)));
  P.on('console', (m) => { if (m.type() === 'error' && !/404|Failed to load resource/.test(m.text())) perr.push(m.text()); });
  // Der Rahmen lädt hier von der gebundenen Shell-URL (stub-lab.html?shell=bound, v4/source/MONACO.md): editor.js löst
  // ctox:lib/monaco@0/ wie im echten Ladeweg auf, die Code-Ansicht läuft mit Monaco. Jede Seitenmeldung zählt.
  const agErrors = () => perr;
  const mut = AG_MUTATIONS[process.env.AG_MUTATE || ''];
  if (mut) {
    const [file, from, to] = mut;
    const re = file === 'index.js' ? /\/v4\/frame\/index\.js(\?|$)/ : new RegExp(`/v4/source/${file.replace('.', '\\.')}(\\?|$)`);
    const disk = path.resolve(here, file === 'index.js' ? '../index.js' : `../../source/${file}`);
    const src = fs.readFileSync(disk, 'utf8');
    if (!src.includes(from)) throw new Error(`AG_MUTATE ${process.env.AG_MUTATE}: Stelle nicht gefunden`);
    await P.route(re, (route) => route.fulfill({ status: 200, contentType: 'text/javascript', body: src.replace(from, to) }));
    console.log(`AG Mutation aktiv: ${process.env.AG_MUTATE} (${file})`);
  }
  // Echte Basis: der Labor-Code von „notes“ wird als Version veröffentlicht (Diff mit Kontext statt leerer Datei);
  // die neue Version lädt ihren Code wie 1.0.0 aus lab/apps/notes/1.0.0.
  const notesDir = path.join(here, 'apps/notes/1.0.0');
  await P.route(/\/~a\/notes@(?!1\.0\.0)[^/]+\//, (route) => {
    const rest = new URL(route.request().url()).pathname.replace(/^.*\/~a\/notes@[^/]+\//, '');
    const f = path.join(notesDir, rest);
    if (!f.startsWith(notesDir) || !fs.existsSync(f)) { route.fulfill({ status: 404 }); return; }
    route.fulfill({ status: 200, contentType: rest.endsWith('.svg') ? 'image/svg+xml' : 'text/javascript', body: fs.readFileSync(f) });
  });
  // Die echte App „Coding Agents“ (modules/coding-agents, Übersicht der Läufe) im Rahmen.
  const caDir = path.resolve(here, '../../../modules/coding-agents');
  await P.route(/\/~a\/coding-agents@[^/]+\//, (route) => {
    const rest = new URL(route.request().url()).pathname.replace(/^.*\/~a\/coding-agents@[^/]+\//, '');
    const f = path.join(caDir, rest);
    if (!f.startsWith(caDir) || !fs.existsSync(f)) { route.fulfill({ status: 404 }); return; }
    const type = rest.endsWith('.css') ? 'text/css' : rest.endsWith('.html') ? 'text/html' : rest.endsWith('.json') ? 'application/json' : rest.endsWith('.svg') ? 'image/svg+xml' : 'text/javascript';
    route.fulfill({ status: 200, contentType: type, body: fs.readFileSync(f) });
  });
  await P.goto(`${base}/v4/frame/lab/stub-lab.html?shell=bound&inst=ag-${Date.now().toString(36)}`);
  await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  const H = (fn, arg) => P.evaluate(fn, arg);
  const caLayout = JSON.parse(fs.readFileSync(path.join(caDir, 'module.json'), 'utf8')).layout;
  // FIXT-R (Pin 4b0d0ac): ohne Routen/collections des echten Manifests sperrt der Kern das Lesen von coding_turns, die App
  // bricht beim Start ab und die Shell schließt ihr Fenster wieder. Eigentümer (ctox-platform) zuerst, dann die eigene Version.
  const caRoutes = realManifestRoutes('coding-agents');
  await H(async ([icon, routes]) => {
    const sim = window.__lab.kernel.sim;
    for (const o of routes._fixture_owners ?? []) if (!(await sim.inspect('business_apps', o.app))?.data?.active_ver) await sim.publishAppVersion(o.app, o.ver, o.publish);
    const { _fixture_owners, ...manifestExtra } = routes;
    await sim.publishAppVersion('coding-agents', '1.0.0', { title: 'Coding Agents', layout: { icon_svg: icon, category: 'Entwicklung', default_width: 520, default_height: 420 }, styles: ['index.css'], manifestExtra });
  }, [caLayout.icon_svg, caRoutes]);
  const seeded = await H(async (text) => {
    const src = window.__lab.kernel.source('notes');
    await src.draft.write('index.js', text);
    return src.draft.publish({}).result;
  }, fs.readFileSync(path.join(notesDir, 'index.js'), 'utf8'));
  if (seeded?.status !== 'accepted') throw new Error(`AG: Basis nicht veröffentlicht: ${JSON.stringify(seeded)}`);
  // Kann der Pin §16.44? (draft.info() trägt rev)
  // Fähigkeiten des Pins: §16.44 (draft.info().rev) und ab 8489b4e draft.write → {local, result} + Kontext-Patch.
  const { supported, writeResult } = await H(async () => {
    const s = window.__lab.kernel.source('tasks');
    await s.draft.rebase(window.__lab.kernel.apps.get().find((a) => a.id === 'tasks').active_ver);
    const i = await s.draft.info();
    const hnd = s.draft.write('probe.txt', 'x');
    await hnd;
    const writeResult = typeof hnd?.result?.then === 'function' ? (await hnd.result)?.status === 'accepted' : false;
    await s.draft.discard();
    return { supported: !!i && 'rev' in i, writeResult };
  });
  console.log(`AG Pin: §16.44 ${supported ? 'ja' : 'nein'}, draft.write.result + Kontext-Patch ${writeResult ? 'ja' : 'nein'}`);
  const open = (id, g) => H(async ([id, g]) => { const h = window.__lab.frame.shellHandle(); const o = h.openWindow(id, { geometry: g }); await h.win(o).host.ready; return o; }, [id, g]);
  const zoneOf = (o) => H((o) => window.__lab.frame.shellHandle().win(o).host.zone.entries().filter((e) => /^agent\./.test(e.code)).map((e) => `${e.level}|${e.code}|${e.text}|${e.action?.label ?? ''}`), o);
  const draftOf = () => H(() => window.__lab.kernel.source('notes').draft.info());
  const agentState = (o) => H((o) => window.__lab.frame.shellHandle().win(o).code?.view?.agent?.state ?? null, o);
  const slow = (ms) => H((ms) => { window.__lab.kernelOpts.codingDelayMs = ms; }, ms); // Laufzeit des simulierten Turns

  const tasks = await open('tasks', { x: 900, y: 40, w: 360, h: 300 });
  const notes = await open('notes', { x: 20, y: 30, w: 860, h: 560 });
  const notesSel = `#${await H((o) => window.__lab.frame.shellHandle().win(o).handle.element.id, notes)}`;
  const input = `${notesSel} [data-agent-input]`;

  // AG-6 Einstieg aus einer anderen App: api.shell.openSource('notes') aus „Aufgaben“ → Code-Ansicht der Ziel-App mit
  // offener Seitenleiste, kein neues Fenster.
  const winsBefore = await P.$$eval('.shell-window', (e) => e.length);
  const r6 = await H((o) => window.__lab.frame.shellHandle().win(o).host.instance.api.shell.openSource('notes'), tasks);
  const panelOk = await P.waitForSelector(`${notesSel}[data-v4-mode="code"] [data-v4-agent="notes"]:not([hidden])`, { timeout: 8000 }).then(() => true, () => false);
  const ph = await P.$eval(input, (i) => i.placeholder).catch(() => '');
  const winsAfter = await P.$$eval('.shell-window', (e) => e.length);
  check('AG-6 Einstieg aus anderer App: openSource(„notes“) aus Aufgaben öffnet die Code-Ansicht der Ziel-App mit offener Agent-Seitenleiste, kein neues Fenster',
    r6 === true && panelOk && winsAfter === winsBefore && ph === 'Was soll der Agent an Notizen ändern?', JSON.stringify({ r6, panelOk, winsBefore, winsAfter, ph }));
  const tEd = Date.now();
  const edReady = await P.waitForSelector(`${notesSel} [data-v4-src-host] .v4-src[data-editor] .v4-src-ed`, { timeout: 15000 }).then(() => Date.now() - tEd, () => null);
  await P.waitForSelector(`${notesSel} [data-v4-src-host] textarea, ${notesSel} [data-v4-src-host] .monaco-editor .view-line`, { timeout: 8000 }).catch(() => {});
  await P.screenshot({ path: shot('ag-panel-1280.png') });
  // AG-M: die Code-Ansicht nutzt den echten Editor (Monaco) über die Lib-Bindung der Shell-Wurzel, nicht den Rückfall.
  const m0 = await H(() => ({
    root: window.__lab.shellRoot,
    frame: performance.getEntriesByType('resource').map((e) => new URL(e.name).pathname).find((n) => n.endsWith('/v4/source/editor.js')) ?? null,
    loader: performance.getEntriesByType('resource').map((e) => new URL(e.name).pathname).find((n) => n.endsWith('/vs/loader.js')) ?? null,
    monaco: !!window.monaco?.editor,
  }));
  m0.edReadyMs = edReady;
  Object.assign(m0, await P.$eval(`${notesSel} [data-v4-src-host] [data-editor]`, (e) => {
    const box = e.querySelector('.v4-src-ed')?.getBoundingClientRect();
    return { editor: e.dataset.editor, path: e.dataset.currentPath ?? null, lines: e.querySelectorAll('.monaco-editor .view-line').length, box: box ? [Math.round(box.width), Math.round(box.height)] : null };
  }).catch(() => ({ editor: null })));
  check('AG-M Code-Ansicht mit Monaco: Rahmen von der Shell-Instanz-URL /~a/shell@lab~lib:…, editor.js löst ctox:lib/monaco@0/ auf /~a/lib:monaco@0.0~lib:ctox-office@1.0/ auf (Umschreibung von v4/sw.js), Editor = monaco',
    m0.root === '/~a/shell@lab~lib:ctox-office@1.0~lib:monaco@0.0/' && m0.frame === `${m0.root}v4/source/editor.js`
      && m0.loader === '/~a/lib:monaco@0.0~lib:ctox-office@1.0/vs/loader.js' && m0.editor === 'monaco' && m0.monaco, JSON.stringify(m0));
  // K-1k (§16.50): Monaco-Worker gleich-ursprünglich unter der Lib-Bindung der Shell (binding.workerUrl), Datei erreichbar.
  const k1k = await H(async () => {
    const url = window.MonacoEnvironment?.getWorkerUrl?.('k1k', 'editorWorkerService') ?? null;
    const k = window.__lab.kernel;
    let bound = null;
    try { bound = typeof k?.shell?.artifacts?.bind === 'function' ? k.shell.artifacts.bind('shell', 'lab').workerUrl('monaco', 'vs/base/worker/workerMain.js') : null; } catch (e) { bound = `Fehler: ${e.message}`; }
    const r = url ? await fetch(url, { method: 'HEAD' }).catch(() => null) : null;
    return { url: url ? new URL(url, location.origin).pathname : null, sameOrigin: url ? new URL(url, location.origin).origin === location.origin : false, bound, status: r?.status ?? null };
  });
  check('K-1k Monaco-Worker gleich-ursprünglich unter der Lib-Bindung (/~a/lib:monaco@0.0~…/vs/base/worker/workerMain.js), erreichbar; im Stub = binding.workerUrl',
    k1k.url === '/~a/lib:monaco@0.0~lib:ctox-office@1.0/vs/base/worker/workerMain.js' && k1k.sameOrigin && k1k.status === 200
      && (!STUB || k1k.bound === k1k.url), JSON.stringify(k1k));
  // Schreibmarke im Editor setzen (Monaco über die Tastatur wie ein Nutzer, sonst textarea): 'start' | 'firstLineEnd' | 'lastLineEnd'.
  const MAC = process.platform === 'darwin';
  const caret = async (where) => {
    const sel = `${notesSel} [data-v4-src-host]`;
    if (await P.$(`${sel} .monaco-editor`)) {
      // In die sichtbare Editorfläche klicken (nicht in die Mitte von .view-lines: die ist so breit wie die längste
      // Zeile, Playwright scrollte sie sonst per scrollIntoView an Monaco vorbei seitlich weg).
      await P.click(`${sel} .v4-src-ed--monaco`, { position: { x: 80, y: 8 } });
      await P.keyboard.press(MAC ? 'Meta+ArrowUp' : 'Control+Home');
      if (where === 'firstLineEnd') await P.keyboard.press('End');
      if (where === 'lastLineEnd') {
        await P.keyboard.press(MAC ? 'Meta+ArrowDown' : 'Control+End');
        if (await H((o) => /\n$/.test(window.__lab.frame.shellHandle().win(o).code.view.editor.getValue()), notes)) await P.keyboard.press('ArrowLeft');
      }
      return;
    }
    await P.$eval(`${sel} textarea`, (t, where) => {
      t.focus();
      const v = t.value.replace(/\n$/, '');
      const i = where === 'start' ? 0 : where === 'firstLineEnd' ? t.value.indexOf('\n') : v.length;
      t.setSelectionRange(i, i);
    }, where);
  };

  if (!supported) {
    // AG-0 Pin ohne §16.44 (vor 51b1e33): Senden meldet „nicht verfügbar“ in die Zone, kein Befehl, Prompt bleibt.
    await P.fill(input, 'Probe');
    await P.press(input, 'Enter');
    await P.waitForTimeout(400);
    const s0 = await agentState(notes);
    const z0 = await zoneOf(notes);
    check('AG-0 Kern ohne §16.44 (Stub-Pin vor 51b1e33): Senden → ein Zoneneintrag „nicht verfügbar“, kein ctox.coding-Befehl, Prompt bleibt',
      s0?.log.length === 0 && z0.length === 1 && /noch nicht verfügbar/.test(z0[0]) && (await P.$eval(input, (i) => i.value)) === 'Probe', JSON.stringify({ s0, z0 }));
    check('AG ohne Seitenfehler', agErrors().length === 0, agErrors().join(' | '));
    await ctx.close();
    return;
  }

  // AG-1 senden: Umschalt+Enter = neue Zeile (kein Befehl), Enter sendet den Vertrag; Turn läuft (Indikator, Abbrechen),
  // am Ende Antwort + Dateiliste, Entwurf geändert (CAS im Kern), geänderte Datei als Diff im Editor.
  await P.click(input);
  await P.keyboard.type('Füge einen Kommentar');
  await P.keyboard.press('Shift+Enter');
  await P.keyboard.type('am Ende von index.js hinzu');
  const afterShift = { value: await P.$eval(input, (i) => i.value), cmds: (await agentState(notes))?.log.length };
  await slow(1200);
  await P.keyboard.press('Enter');
  const runningSeen = await P.waitForSelector(`${notesSel} .v4-agent__turn[data-state="running"] .v4-agent__dots, ${notesSel} .v4-agent__turn[data-state="queued"] .v4-agent__dots`, { timeout: 3000 }).then(() => true, () => false);
  const cancelVisible = await P.$eval(`${notesSel} [data-agent-run]`, (b) => b.dataset.agentRun === 'cancel' && b.textContent === 'Abbrechen' && b.getAttribute('aria-busy') === 'true' && b.classList.contains('ctox-run-control')).catch(() => false);
  await slow(60);
  await P.waitForSelector(`${notesSel} .v4-agent__turn[data-state="done"]`, { timeout: 8000 }).catch(() => {});
  await P.waitForSelector(`${notesSel} [data-v4-text-diff]`, { timeout: 4000 }).catch(() => {});
  const s1 = await agentState(notes);
  const d1 = await draftOf();
  const ui1 = await P.$eval(notesSel, (w) => ({
    reply: w.querySelector('.v4-agent__turn[data-state="done"] .v4-agent__reply')?.textContent ?? null,
    files: [...w.querySelectorAll('.v4-agent__turn[data-state="done"] [data-agent-file]')].map((b) => b.dataset.agentFile),
    view: w.querySelector('.v4-src__pane')?.dataset.view, diffPath: w.querySelector('[data-diff-path]')?.dataset.diffPath ?? null,
    added: w.querySelector('[data-v4-text-diff]')?.dataset.added ?? null, diffPressed: w.querySelector('[data-diff-toggle]')?.getAttribute('aria-pressed'),
    input: w.querySelector('[data-agent-input]').value, cancelHidden: w.querySelector('[data-agent-run]').dataset.agentRun === 'send',
    pub: !w.querySelector('[data-publish]').hidden && !w.querySelector('[data-publish]').disabled, discard: !w.querySelector('[data-discard]').hidden,
  }));
  const l1 = s1?.log ?? [];
  const p1 = l1[0]?.payload ?? {};
  const agentFile = ui1.files[0];
  check('AG-1 senden: Umschalt+Enter macht eine neue Zeile (kein Befehl), Enter sendet ctox.coding.turn {app_id, draft_id, base_draft_rev = draft.info().rev, prompt, turn_id}; Indikator + Abbrechen während des Laufs; danach Antwort, Dateiliste, Entwurf per CAS geändert, Datei als Diff geöffnet, Veröffentlichen/Verwerfen bereit',
    afterShift.value === 'Füge einen Kommentar\nam Ende von index.js hinzu' && afterShift.cmds === 0
      && l1.length === 1 && l1[0].type === 'ctox.coding.turn' && l1[0].status === 'running' && p1.app_id === 'notes' && p1.draft_id === 'owner/notes' && typeof p1.base_draft_rev === 'string' && p1.prompt === afterShift.value && /^turn-/.test(p1.turn_id)
      && runningSeen && cancelVisible && ui1.reply && agentFile && new RegExp(`coding-agent ${p1.turn_id}: Füge einen Kommentar`).test(d1?.files?.[agentFile] ?? '')
      && ui1.view === 'diff' && ui1.diffPath === agentFile && Number(ui1.added) >= 1 && ui1.diffPressed === 'true' && ui1.input === '' && ui1.cancelHidden && ui1.pub && ui1.discard,
    JSON.stringify({ afterShift, l1, runningSeen, cancelVisible, ui1, draftFile: agentFile && d1?.files?.[agentFile]?.slice(-90) }));
  await P.screenshot({ path: shot('ag-diff-1280.png') });

  // AG-2 turn_running: zweiter Auftrag während ein Turn läuft → Kern lehnt ab, Hinweis in die Zone, Prompt bleibt.
  await slow(2500);
  await P.fill(input, 'Erster Auftrag');
  await P.press(input, 'Enter');
  await P.waitForSelector(`${notesSel} .v4-agent__turn[data-state="running"]`, { timeout: 3000 }).catch(() => {});
  await P.fill(input, 'Zweiter Auftrag');
  await P.press(input, 'Enter');
  await P.waitForTimeout(300);
  const s2 = await agentState(notes);
  const z2 = await zoneOf(notes);
  const in2 = await P.$eval(input, (i) => i.value);
  check('AG-2 turn_running: zweiter Auftrag während eines laufenden Turns → Kern lehnt ab (rejected conflict turn_running), Hinweis in der Zone, Prompt bleibt im Feld, genau ein laufender Turn',
    s2.log.at(-2)?.status === 'running' && s2.log.at(-1)?.status === 'rejected' && s2.log.at(-1)?.code === 'conflict' && s2.log.at(-1)?.reason === 'turn_running'
      && s2.turns.filter((t) => ['queued', 'running'].includes(t.state)).length === 1 && in2 === 'Zweiter Auftrag'
      && z2.length === 1 && /^warn\|agent\.turn\|Der Agent arbeitet noch/.test(z2[0]), JSON.stringify({ log: s2.log.map((c) => [c.type, c.status, c.reason]), turns: s2.turns, in2, z2 }));
  await P.screenshot({ path: shot('ag-running-1280.png') });

  // AG-5 abbrechen: „Abbrechen“ → ctox.coding.cancel; Turn cancelled, Indikator weg, Entwurf unverändert.
  const before5 = await draftOf();
  await P.click(`${notesSel} [data-agent-run="cancel"]`);
  await P.waitForSelector(`${notesSel} .v4-agent__turn[data-state="cancelled"]`, { timeout: 3000 }).catch(() => {});
  await slow(60);
  await P.waitForTimeout(2800); // der abgebrochene Turn schläft noch und darf danach nichts schreiben
  const s5 = await agentState(notes);
  const after5 = await draftOf();
  const ui5 = await P.$eval(notesSel, (w) => ({ dots: !!w.querySelector('.v4-agent__dots'), cancel: w.querySelector('[data-agent-run]').dataset.agentRun === 'cancel', send: w.querySelector('[data-agent-run]').textContent === 'Senden' && !w.querySelector('[data-agent-run]').hasAttribute('aria-busy') }));
  const cancelled = s5.turns.find((t) => t.state === 'cancelled');
  check('AG-5 abbrechen: Knopf sendet ctox.coding.cancel {turn_id}; Turn cancelled (auch nach Ablauf der Arbeit), kein Indikator, Senden wieder da, Entwurf unverändert',
    s5.log.at(-1)?.type === 'ctox.coding.cancel' && s5.log.at(-1)?.status === 'completed' && s5.log.at(-1)?.payload.turn_id === cancelled?.id
      && s5.turns.filter((t) => t.state === 'cancelled').length === 1 && !ui5.dots && !ui5.cancel && ui5.send && JSON.stringify(before5.files) === JSON.stringify(after5.files) && before5.rev === after5.rev,
    JSON.stringify({ last: s5.log.at(-1), turns: s5.turns, ui5 }));
  await H((o) => window.__lab.frame.shellHandle().win(o).host.zone.clear('agent.turn'), notes);

  // AG-3 stale_draft_rev: eigene Eingabe im Editor und sofort Senden (offline, damit Entwurfsänderung und Befehl
  // gemeinsam im Ausgang liegen) → Kern lehnt den Befehl mit stale_draft_rev ab; die Seitenleiste lädt still neu,
  // wartet auf die bestätigte Revision und sendet genau einmal erneut; kein Zoneneintrag; eigene Änderung bleibt.
  if (await P.$eval(`${notesSel} [data-diff-toggle]`, (b) => b.getAttribute('aria-pressed') === 'true')) await P.click(`${notesSel} [data-diff-toggle]`);
  await P.click(`${notesSel} .v4-src__item[data-path="index.js"]`);
  const ta = `${notesSel} [data-v4-src-host] .v4-src-ed`;
  await P.waitForSelector(ta, { timeout: 6000 });
  await H(() => window.__lab.kernel.sim.setLink({ state: 'offline' }));
  await caret('start');
  await P.keyboard.type('/* vor dem Auftrag */ ');
  const n3 = (await agentState(notes)).log.length;
  await P.fill(input, 'Nach eigener Änderung in index.js');
  await P.press(input, 'Enter');
  await P.waitForTimeout(300);
  await H(() => window.__lab.kernel.sim.setLink({ state: 'online' }));
  await P.waitForFunction(([n, k]) => { const s = window.__lab.frame.shellHandle().windows().find((w) => w.appId === 'notes')?.code?.view?.agent?.state; return s && s.log.length >= n + k; }, [n3, writeResult ? 1 : 2], { timeout: 8000 }).catch(() => {});
  await P.waitForFunction(() => window.__lab.frame.shellHandle().windows().find((w) => w.appId === 'notes')?.code?.view?.agent?.state.turns.some((t) => t.state === 'done' && t.id), null, { timeout: 8000 }).catch(() => {});
  await P.waitForTimeout(400);
  const s3 = await agentState(notes);
  const d3 = await draftOf();
  const tail3 = s3.log.slice(n3).map((c) => [c.status, c.reason, c.revSource]);
  const z3 = await zoneOf(notes);
  // Kern ≥ 8489b4e: die Seitenleiste wartet auf draft.write(...).result und sendet mit result.rev → keine Ablehnung.
  // Ältere Kerne: Befehl mit veralteter Revision → stale_draft_rev → still neu laden, genau ein Wiederholversuch.
  const want3 = writeResult ? [['running', null, 'write']] : [['rejected', 'stale_draft_rev', 'info'], ['running', null, 'retry']];
  check(writeResult
    ? 'AG-3 eigene Eingabe vor dem Senden: Seitenleiste wartet auf draft.write(...).result und sendet base_draft_rev = result.rev (keine stale-Ablehnung, kein Zoneneintrag); eigene Änderung und Agent-Zeile im Entwurf'
    : 'AG-3 stale_draft_rev (alter Kern ohne write.result): Befehl mit veralteter Revision → still neu geladen, auf bestätigte Revision gewartet, genau einmal erneut gesendet; kein Zoneneintrag; eigene Änderung und Agent-Zeile im Entwurf',
    JSON.stringify(tail3) === JSON.stringify(want3) && z3.length === 0
      && /^\/\* vor dem Auftrag \*\//.test(d3?.files?.['index.js'] ?? '') && /Nach eigener Änderung/.test(d3?.files?.['index.js'] ?? '') && (await P.$eval(input, (i) => i.value)) === '',
    JSON.stringify({ tail3, z3, head: d3?.files?.['index.js']?.slice(0, 40), tail: d3?.files?.['index.js']?.slice(-80) }));

  // AG-4 Konflikt: während der Turn läuft, tippt der Nutzer im Editor → Kern schreibt nicht, Patch-Datei;
  // „Änderungen übernehmen“ legt die Agent-Änderung über die eigene, Diff öffnet sich, Zone geräumt.
  await slow(1500);
  await P.fill(input, 'Konfliktfall in index.js');
  await P.press(input, 'Enter');
  await P.waitForSelector(`${notesSel} .v4-agent__turn[data-state="running"]`, { timeout: 3000 }).catch(() => {});
  if (await P.$eval(`${notesSel} [data-diff-toggle]`, (b) => b.getAttribute('aria-pressed') === 'true')) await P.click(`${notesSel} [data-diff-toggle]`);
  await P.click(`${notesSel} .v4-src__item[data-path="index.js"]`);
  await P.waitForSelector(ta, { timeout: 6000 });
  if (writeResult) {
    // Kontext-Patch (Kern ≥ 8489b4e): eigene Zeilen OBEN einfügen – der Patch muss trotzdem unten richtig landen.
    await caret('start');
    await P.keyboard.type('// eigene Änderung oben 1\n// eigene Änderung oben 2\n');
  } else {
    // Null-Kontext-Patch (alter Kern): eigene Änderung am Ende der ERSTEN Zeile (Zeilenzahl bleibt).
    await caret('firstLineEnd');
    await P.keyboard.type(' /* eigene Änderung */');
  }
  await H(async (o) => { await window.__lab.frame.shellHandle().win(o).code?.view?.flush?.(); }, notes);
  await slow(60);
  await P.waitForSelector(`${notesSel} [data-agent-apply]`, { timeout: 8000 }).catch(() => {});
  const d4 = await draftOf();
  const z4 = await zoneOf(notes);
  const tr4 = (await agentState(notes)).turns.find((t) => t.state === 'conflict');
  const pre4 = { own: /eigene Änderung/.test(d4?.files?.['index.js'] ?? ''), agentIn: /Konfliktfall/.test(d4?.files?.['index.js'] ?? ''), z4, patch: tr4?.patch?.file_id };
  await P.screenshot({ path: shot('ag-conflict-1280.png') });
  await P.click(`${notesSel} [data-agent-apply]`).catch(() => {});
  await P.waitForSelector(`${notesSel} [data-v4-text-diff]`, { timeout: 4000 }).catch(() => {});
  await P.waitForTimeout(300);
  const d4b = await draftOf();
  const lines4 = (d4b?.files?.['index.js'] ?? '').trimEnd().split('\n');
  const post4 = { own: writeResult ? lines4[0] === '// eigene Änderung oben 1' && lines4[1] === '// eigene Änderung oben 2' : /eigene Änderung/.test(lines4[0] ?? ''), agentLast: /coding-agent .*Konfliktfall/.test(lines4.at(-1) ?? ''), zone: await zoneOf(notes),
    apply: await P.$$eval(`${notesSel} [data-agent-apply]`, (e) => e.length), view: await P.$eval(`${notesSel} .v4-src__pane`, (p) => p.dataset.view) };
  check(`AG-4 Konflikt: Entwurf bleibt beim Turn-Ende unangetastet (eigene Änderung drin, Agent nicht), Patch files/coding-patch:<turn_id>, Zone warn mit „Änderungen übernehmen“; Übernehmen setzt die Agent-Zeile ans Ende ${writeResult ? 'trotz zweier oben eingefügter eigener Zeilen (Kontext-Patch)' : 'über die eigene Änderung in Zeile 1'}, Diff offen, Zone leer`,
    pre4.own && !pre4.agentIn && pre4.patch === `coding-patch:${tr4?.id}` && pre4.z4.length === 1 && /^warn\|agent\.conflict\|.*\|Änderungen übernehmen$/.test(pre4.z4[0])
      && post4.own && post4.agentLast && post4.zone.length === 0 && post4.apply === 0 && post4.view === 'diff', JSON.stringify({ pre4, post4 }));

  // AG-4c (nur Kontext-Patch): Nutzer ändert während des Turns eine Kontextzeile des Patches → Übernehmen lehnt ab,
  // Zone agent.patch, Entwurf unverändert, Knopf bleibt (nie stumm daneben).
  if (writeResult) {
    await slow(1500);
    await P.fill(input, 'Kontextbruch in index.js');
    await P.press(input, 'Enter');
    await P.waitForSelector(`${notesSel} .v4-agent__turn[data-state="running"]`, { timeout: 3000 }).catch(() => {});
    if (await P.$eval(`${notesSel} [data-diff-toggle]`, (b) => b.getAttribute('aria-pressed') === 'true')) await P.click(`${notesSel} [data-diff-toggle]`);
    await P.click(`${notesSel} .v4-src__item[data-path="index.js"]`);
    await P.waitForSelector(ta, { timeout: 6000 });
    await caret('lastLineEnd');
    await P.keyboard.type(' /* letzte Zeile geändert */');
    await H(async (o) => { await window.__lab.frame.shellHandle().win(o).code?.view?.flush?.(); }, notes);
    await slow(60);
    await P.waitForSelector(`${notesSel} .v4-agent__turn[data-state="conflict"] [data-agent-apply]`, { timeout: 8000 }).catch(() => {});
    await H((o) => window.__lab.frame.shellHandle().win(o).host.zone.clear('agent.conflict'), notes);
    const before4c = (await draftOf())?.files?.['index.js'];
    await P.click(`${notesSel} [data-agent-apply]`).catch(() => {});
    await P.waitForTimeout(500);
    const after4c = (await draftOf())?.files?.['index.js'];
    const z4c = await zoneOf(notes);
    const btn4c = await P.$$eval(`${notesSel} [data-agent-apply]`, (e) => e.length);
    check('AG-4c Kontext passt nicht (eigene Änderung an einer Kontextzeile): Übernehmen lehnt ab → genau ein Zoneneintrag agent.patch „passen nicht mehr“, Entwurf unverändert, „Änderungen übernehmen“ bleibt',
      before4c === after4c && /letzte Zeile geändert/.test(after4c ?? '') && !/Kontextbruch/.test(after4c ?? '') && z4c.length === 1 && /^error\|agent\.patch\|Agent-Änderungen passen nicht mehr/.test(z4c[0]) && btn4c === 1,
      JSON.stringify({ z4c, btn4c, same: before4c === after4c, tail: after4c?.slice(-120) }));
    await H((o) => window.__lab.frame.shellHandle().win(o).host.zone.clear('agent.patch'), notes);
  }

  // AG-7 Einstieg aus „Coding Agents“: die App listet die Läufe (coding_turns), Kopf = Standard-Icon-Header der Shell;
  // Klick auf einen Lauf öffnet die Code-Ansicht der App mit offener Agent-Seitenleiste.
  const nTurns = (await agentState(notes)).turns.length;
  await H((o) => window.__lab.frame.shellHandle().win(o).handle.minimize?.(), notes);
  const ca = await open('coding-agents', { x: 40, y: 60, w: 560, h: 440 });
  const caSel = `#${await H((o) => window.__lab.frame.shellHandle().win(o).handle.element.id, ca)}`;
  await P.waitForSelector(`${caSel} [data-ca-run]`, { timeout: 6000 }).catch(() => {});
  await P.waitForTimeout(300);
  const ca7 = await P.$eval(caSel, (w) => {
    const icon = w.querySelector('.shell-window-v2-icon')?.getBoundingClientRect();
    const title = w.querySelector('.ctox-pane-title')?.getBoundingClientRect();
    return {
      runs: [...w.querySelectorAll('[data-ca-run]')].map((b) => ({ app: b.dataset.app, state: b.dataset.state, text: b.querySelector('.coding-agents-run-prompt')?.textContent })),
      ownMark: !!w.querySelector('.coding-agents-rail-mark, .coding-agents-rail-spacer'),
      iconBox: icon && { l: Math.round(icon.left), r: Math.round(icon.right), b: Math.round(icon.bottom) },
      titleBox: title && { l: Math.round(title.left), t: Math.round(title.top), text: w.querySelector('.ctox-pane-title').textContent },
    };
  });
  await P.screenshot({ path: shot('ag-coding-agents-1280.png') });
  await P.click(`${caSel} [data-ca-run][data-app="notes"]`).catch(() => {});
  const ok7 = await P.waitForSelector(`${notesSel}[data-v4-mode="code"] [data-v4-agent="notes"]:not([hidden])`, { timeout: 6000 }).then(() => true, () => false);
  const vis7 = await P.$eval(notesSel, (w) => getComputedStyle(w).display !== 'none' && w.getBoundingClientRect().width > 0).catch(() => false);
  check('AG-7 „Coding Agents“ = Übersicht der Läufe aus coding_turns (alle angelegten Läufe der Sitzung – Ablehnungen legen keinen an –, Auftrag je Zeile), Kopf ohne eigenes Icon, Titel rechts neben dem Shell-Icon; Klick öffnet die Code-Ansicht von „Notizen“ mit Agent-Seitenleiste',
    ca7.runs.length === nTurns && nTurns === (writeResult ? 5 : 4) && ca7.runs.every((r) => r.app === 'notes') && ca7.runs.some((r) => r.text === 'Konfliktfall in index.js') && ca7.runs.some((r) => r.state === 'cancelled' && r.text === 'Erster Auftrag') && !ca7.ownMark
      && ca7.iconBox && ca7.titleBox && ca7.titleBox.l >= ca7.iconBox.r && ok7 && vis7, JSON.stringify({ nTurns, ca7, ok7, vis7 }));
  await H((o) => window.__lab.frame.shellHandle().closeWindow(o), ca);

  // Einklappen/Einblenden und Bildschirmbilder 1280 / 900.
  await P.click(`${notesSel} [data-agent-collapse]`);
  const collapsed = await P.$eval(`${notesSel} [data-v4-agent]`, (a) => a.hidden) && await P.$eval(`${notesSel} [data-agent-toggle]`, (b) => !b.hidden);
  await P.screenshot({ path: shot('ag-collapsed-1280.png') });
  await P.click(`${notesSel} [data-agent-toggle]`);
  const reopened = await P.$eval(`${notesSel} [data-v4-agent]`, (a) => !a.hidden);
  check('AG Seitenleiste einklappbar: › blendet aus (Lasche „Agent“ rechts), Lasche blendet wieder ein', collapsed && reopened, JSON.stringify({ collapsed, reopened }));
  await P.setViewportSize({ width: 900, height: 700 });
  await H((o) => { const h = window.__lab.frame.shellHandle(); h.win(o).handle.element.style.width = '860px'; h.win(o).handle.element.style.left = '10px'; }, notes);
  await P.waitForTimeout(300);
  await P.screenshot({ path: shot('ag-panel-900.png') });
  await P.setViewportSize({ width: 1280, height: 800 });
  check('AG ohne Seitenfehler', agErrors().length === 0, agErrors().join(' | '));
  await ctx.close();
}

// ================= LOOP (Strang LOOP, Bedienkonzept §3a): Melden → Meldungen → Agent → Veröffentlichen → Rückmeldung =================
// Eigene Seite, nur gegen den Kern-Stub ab §16.44 + draft.write.result (Stub-Pin 8489b4e); ältere Pins: Block entfällt.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/8489b4e LOOP_ONLY=1 node …/frame.test.mjs stub
// LOOP_MUTATE=<name> liefert eine mutierte Datei aus (Mutationsbeleg: der zugehörige Fall muss rot werden).
const LOOP_MUTATIONS = {
  melden: ['shell/report.js', 'id, report_id: id, module_id: win.appId,', "id, report_id: id, module_id: '',"], // LOOP-1
  filter: ['source/reports-tab.js', 'api.query(REPORT_COLLECTIONS.reports, { selector: { module_id: appId } })', 'api.query(REPORT_COLLECTIONS.reports, {})'], // LOOP-2
  link: ['source/source-mode.js', "await linkReportToTurn(reportsApi, rep, { turn_id: r.turn_id, command_id: r.command_id ?? '', draft_id: r.draft_id ?? '' });", ''], // LOOP-3
  note: ['shell/report.js', "return `behebt ${list.length === 1 ? 'Meldung' : 'Meldungen'} ${list.map((n) => `#${n}`).join(', ')}`;", "return '';"], // LOOP-4
  seen: ['shell/report.js', 'try { api.patch(REPORT_COLLECTIONS.reports, r.id, { reporter_seen_at_ms: Date.now() }); } catch { /* still */ }', ''], // LOOP-5
  // Runde 2
  uebergabe: ['../modules/reports/index.js', "return { appId: report.moduleId, options: { tab: 'reports', reportId: report.rowId || report.id } };", 'return { appId: report.moduleId, options: undefined };'], // LOOP-6
  durchreichen: ['shell/app-host.js', 'const ok = !!frame.openSource?.(id, focus);', 'const ok = !!frame.openSource?.(id);'], // LOOP-6, LOOP-7
  markieren: ['source/reports-tab.js', 'if (st.marked === r.id) {', 'if (false) {'], // LOOP-6, LOOP-7
  name: ['source/reports-tab.js', 'if (fromDir) return fromDir;', 'if (fromDir) return id;'], // LOOP-8
  rueckfall: ['source/reports-tab.js', 'return stored && stored !== id ? stored : REPORTER_FALLBACK;', 'return stored || id;'], // LOOP-8
};
// Pin 4b0d0ac: Fremdlesen der Shell (business_module_reports, business_users, … Eigentümer ctox) braucht ein aktives
// Eigentümer-Manifest mit collections + exports. stub-lab.html veröffentlicht Eigentümer bewusst nicht (der Fall, der sie
// braucht, tut es) – Muster _fixture_owners aus v4/tools/port/real-manifest.mjs.
async function publishShellOwners(H, apps) {
  const owners = (realManifestRoutes('shell')._fixture_owners ?? []).filter((o) => apps.includes(o.app));
  await H(async (owners) => {
    const sim = window.__lab.kernel.sim;
    for (const o of owners) if (!(await sim.inspect('business_apps', o.app))?.data?.active_ver) await sim.publishAppVersion(o.app, o.ver, o.publish);
  }, owners);
}

async function loop() {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const P = await ctx.newPage();
  const perr = [];
  P.on('pageerror', (e) => perr.push(String(e)));
  P.on('console', (m) => { if (m.type() === 'error' && !/404|Failed to load resource/.test(m.text())) perr.push(m.text()); });
  const loopErrors = () => perr.filter((e) => !/Fetch API cannot load ctox:lib\/monaco/.test(e));
  const mut = LOOP_MUTATIONS[process.env.LOOP_MUTATE || ''];
  // Die echte App „Bugs & Features“ (modules/reports) im Rahmen (Runde 2, LOOP-6); ctox:shared/ wie der Service Worker.
  const repDir = path.resolve(here, '../../../modules/reports');
  const repMut = mut && mut[0].startsWith('../modules/reports/') ? mut : null;
  await P.route(/\/~a\/reports@[^/]+\//, (route) => {
    const rest = new URL(route.request().url()).pathname.replace(/^.*\/~a\/reports@[^/]+\//, '');
    const f = path.join(repDir, rest);
    if (!f.startsWith(repDir) || !fs.existsSync(f)) { route.fulfill({ status: 404 }); return; }
    const type = rest.endsWith('.css') ? 'text/css' : rest.endsWith('.html') ? 'text/html' : rest.endsWith('.json') ? 'application/json' : rest.endsWith('.svg') ? 'image/svg+xml' : 'text/javascript';
    let body = fs.readFileSync(f, 'utf8');
    if (type === 'text/javascript') body = body.replace(/(['"])ctox:shared\//g, '$1/shared/');
    if (repMut && `../modules/reports/${rest}` === repMut[0]) {
      if (!body.includes(repMut[1])) throw new Error(`LOOP_MUTATE ${process.env.LOOP_MUTATE}: Stelle nicht gefunden`);
      body = body.replace(repMut[1], repMut[2]);
    }
    route.fulfill({ status: 200, contentType: type, body });
  });
  if (repMut) console.log(`LOOP Mutation aktiv: ${process.env.LOOP_MUTATE} (${repMut[0]})`);
  if (mut && !repMut) {
    const [file, from, to] = mut;
    const disk = path.resolve(here, '../..', file);
    const src = fs.readFileSync(disk, 'utf8');
    if (!src.includes(from)) throw new Error(`LOOP_MUTATE ${process.env.LOOP_MUTATE}: Stelle nicht gefunden`);
    await P.route(new RegExp(`/v4/${file.replace('.', '\\.')}(\\?|$)`), (route) => route.fulfill({ status: 200, contentType: 'text/javascript', body: src.replace(from, to) }));
    console.log(`LOOP Mutation aktiv: ${process.env.LOOP_MUTATE} (${file})`);
  }
  const notesDir = path.join(here, 'apps/notes/1.0.0');
  await P.route(/\/~a\/notes@(?!1\.0\.0)[^/]+\//, (route) => {
    const rest = new URL(route.request().url()).pathname.replace(/^.*\/~a\/notes@[^/]+\//, '');
    const f = path.join(notesDir, rest);
    if (!f.startsWith(notesDir) || !fs.existsSync(f)) { route.fulfill({ status: 404 }); return; }
    route.fulfill({ status: 200, contentType: rest.endsWith('.svg') ? 'image/svg+xml' : 'text/javascript', body: fs.readFileSync(f) });
  });
  await P.goto(`${base}/v4/frame/lab/stub-lab.html?inst=loop-${Date.now().toString(36)}`);
  await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  const H = (fn, arg) => P.evaluate(fn, arg);
  await publishShellOwners(H, ['ctox']);
  const caps = await H(async () => {
    const s = window.__lab.kernel.source('tasks');
    await s.draft.rebase(window.__lab.kernel.apps.get().find((a) => a.id === 'tasks').active_ver);
    const i = await s.draft.info();
    const hnd = s.draft.write('probe.txt', 'x');
    await hnd;
    const wr = typeof hnd?.result?.then === 'function' ? (await hnd.result)?.status === 'accepted' : false;
    await s.draft.discard();
    return { rev: !!i && 'rev' in i, wr };
  });
  if (!caps.rev || !caps.wr) { console.log('LOOP: Pin ohne §16.44/draft.write.result – Block entfällt'); await ctx.close(); return; }
  const seeded = await H(async (text) => { const src = window.__lab.kernel.source('notes'); await src.draft.write('index.js', text); return src.draft.publish({}).result; }, fs.readFileSync(path.join(notesDir, 'index.js'), 'utf8'));
  if (seeded?.status !== 'accepted') throw new Error(`LOOP: Basis nicht veröffentlicht: ${JSON.stringify(seeded)}`);
  const open = (id, g) => H(async ([id, g]) => { const h = window.__lab.frame.shellHandle(); const o = h.openWindow(id, { geometry: g }); await h.win(o).host.ready; return o; }, [id, g]);
  const reportsDb = () => H(async () => {
    const api = window.__lab.kernel.appApi('ctox', '1');
    const lq = api.query('business_module_reports', {});
    for (let i = 0; i < 30 && !(lq.get() || []).length; i++) await new Promise((r) => setTimeout(r, 50));
    return (lq.get() || []).map((r) => ({ id: r.id, module_id: r.module_id, kind: r.kind, title: r.title, status: r.status, coding_turn_id: r.coding_turn_id ?? null, fixed_in_ver: r.fixed_in_ver ?? null, fixed_note: r.fixed_note ?? null, reporter_seen_at_ms: r.reporter_seen_at_ms ?? null, reporter_id: r.reporter_id }));
  });
  const tasks = await open('tasks', { x: 900, y: 40, w: 360, h: 300 });
  let notes = await open('notes', { x: 20, y: 30, w: 860, h: 560 });
  const NWL = '.shell-window[data-window-app="notes"]';
  const TWL = '.shell-window[data-window-app="tasks"]';
  async function melde(sel, appId, kind, what, expected) {
    await H(([a, k]) => { window.__loopReport = window.__lab.frame.reportProblem({ appId: a, kind: k }); }, [appId, kind]);
    await P.waitForSelector(`${sel} [data-v4-sheet]`);
    await P.fill(`${sel} [data-v4-sheet-input]`, what);
    await P.click(`${sel} [data-v4-sheet-ok]`);
    await P.waitForTimeout(100);
    await P.fill(`${sel} [data-v4-sheet-input]`, expected);
    await P.click(`${sel} [data-v4-sheet-ok]`);
    return H(async () => (await window.__loopReport)?.report_id ?? null);
  }
  await H((o) => { const r = window.__lab.frame.shellHandle().win(o); r.host.zone.report({ level: 'error', code: 'lab.render', text: 'Liste nicht gerendert', source: 'app' }); }, notes);
  const bugId = await melde(NWL, 'notes', 'bug', 'Die Liste springt beim Speichern nach oben', 'Die Liste bleibt an ihrer Stelle');
  await melde(TWL, 'tasks', 'bug', 'Aufgabe lässt sich nicht abhaken', 'Haken setzt den Status');
  // Code-Ansicht der App öffnen, Reiter „Meldungen“
  await H((o) => window.__lab.frame.shellHandle().win(o).host.instance.api.shell.openSource('notes'), notes);
  await P.waitForSelector(`${NWL}[data-v4-mode="code"] [data-src-tab="reports"]`, { timeout: 8000 }).catch(() => {});
  await P.click(`${NWL} [data-src-tab="reports"]`).catch(() => {});
  await P.waitForTimeout(150);
  // LOOP-1: Melden aus dem Fenster, während die Code-Ansicht offen ist → die neue Meldung erscheint live im Reiter.
  const wishId = await (async () => {
    await H(() => { window.__loopReport = window.__lab.frame.reportProblem({ appId: 'notes', kind: 'wish' }); });
    await P.waitForSelector(`${NWL} [data-v4-sheet]`);
    await P.fill(`${NWL} [data-v4-sheet-input]`, 'Spalte „Fällig“ sortierbar');
    await P.click(`${NWL} [data-v4-sheet-ok]`);
    await P.waitForTimeout(100);
    await P.fill(`${NWL} [data-v4-sheet-input]`, 'Klick auf den Kopf sortiert');
    await P.click(`${NWL} [data-v4-sheet-ok]`);
    return H(async () => (await window.__loopReport)?.report_id ?? null);
  })();
  await P.waitForTimeout(300);
  const rows = () => P.$$eval(`${NWL} [data-v4-reports] [data-report-id]`, (els) => els.map((e) => ({
    id: e.dataset.reportId, no: e.dataset.reportNo, status: e.dataset.status, title: e.querySelector('.v4-src__report-title')?.textContent,
    kind: e.querySelector('.v4-src__report-kind')?.textContent, by: e.querySelector('.v4-src__report-by')?.textContent, date: e.querySelector('.v4-src__report-date')?.textContent,
    st: e.querySelector('.v4-src__report-status')?.textContent, assign: !!e.querySelector('[data-report-assign]'), turn: e.querySelector('[data-report-turn]')?.dataset.reportTurn ?? null,
  })));
  const r1 = await rows();
  const db1 = await reportsDb();
  const tab1 = await P.$eval(`${NWL} [data-src-tab="reports"]`, (b) => ({ sel: b.getAttribute('aria-selected'), count: b.dataset.count })).catch(() => null);
  check('LOOP-1 Melden aus dem Fenster: „Wunsch äußern“ bei offener Code-Ansicht speichert die Meldung mit module_id der App; sie erscheint sofort im Reiter „Meldungen“ (Zahl am Reiter 2)',
    !!wishId && db1.find((r) => r.id === wishId)?.module_id === 'notes' && r1.some((r) => r.id === wishId && r.kind === 'Wunsch') && tab1?.count === '2',
    JSON.stringify({ wishId, tab1, r1: r1.map((r) => r.id), db: db1.map((r) => `${r.id}:${r.module_id}`) }));
  await P.screenshot({ path: shot('loop-meldungen-1280.png') });
  // LOOP-2: nur die offenen Meldungen genau dieser App, mit Nummer, Titel, Art, Melder, Datum, Status.
  const today = new Intl.DateTimeFormat('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric' }).format(new Date());
  const bugRow = r1.find((r) => r.id === bugId);
  check('LOOP-2 Reiter „Meldungen“: genau die 2 offenen Meldungen von Notizen (nicht die von Aufgaben), älteste zuerst #1/#2, je Titel, Art (Problem/Wunsch), Melder, Datum, Status „Offen“ und „Agent beauftragen“',
    r1.length === 2 && r1[0].id === bugId && r1[0].no === '1' && r1[1].no === '2' && bugRow?.title === 'Die Liste springt beim Speichern nach oben' && bugRow?.kind === 'Problem' && bugRow?.by === 'Du'
      && bugRow?.date === today && r1.every((r) => r.st === 'Offen' && r.assign) && tab1?.sel === 'true',
    JSON.stringify({ r1, today }));
  // LOOP-3: „Agent beauftragen“ → ctox.coding.turn in §16.44-Form mit Meldung im Auftrag; Meldung „in Arbeit“ + Lauf verlinkt.
  await H(() => { window.__lab.kernelOpts.codingDelayMs = 600; });
  await P.click(`${NWL} [data-report-assign="${bugId}"]`);
  await P.waitForSelector(`${NWL} [data-v4-reports] [data-report-id="${bugId}"][data-status="running"] [data-report-turn]`, { timeout: 6000 }).catch(() => {});
  const ag3 = await H((o) => window.__lab.frame.shellHandle().win(o).code?.view?.agent?.state ?? null, notes);
  const l3 = ag3?.log?.at(-1) ?? {};
  const p3 = l3.payload ?? {};
  const db3 = (await reportsDb()).find((r) => r.id === bugId) ?? {};
  const r3 = (await rows()).find((r) => r.id === bugId) ?? {};
  const cmd3 = await H((tid) => window.__lab.kernel.appApi('ctox-platform', '1').query('business_commands', {}).get()?.find((c) => c.payload?.turn_id === tid)?.record_id ?? null, p3.turn_id);
  await P.click(`${NWL} [data-report-turn]`).catch(() => {});
  await P.waitForTimeout(100);
  const focused = await P.$eval(`${NWL} .v4-agent__turn.is-focused`, (c) => c.dataset.turnId).catch(() => null);
  check('LOOP-3 „Agent beauftragen“: ctox.coding.turn {app_id, base_draft_rev, draft_id, prompt, turn_id} (§16.44, record_id = Meldung); Auftrag = Meldungstext + Erwartung + App/Version + Zonenfehler; Meldung „In Arbeit“ mit coding_turn_id, „Lauf ansehen“ hebt den Turn in der Seitenleiste hervor',
    l3.type === 'ctox.coding.turn' && l3.status === 'running' && JSON.stringify(Object.keys(p3).sort()) === JSON.stringify(['app_id', 'base_draft_rev', 'draft_id', 'prompt', 'turn_id'])
      && p3.app_id === 'notes' && /Meldung #1 \(Problem\)/.test(p3.prompt) && /springt beim Speichern/.test(p3.prompt) && /Erwartet: Die Liste bleibt/.test(p3.prompt) && /App: Notizen · Version /.test(p3.prompt) && /lab\.render/.test(p3.prompt)
      && db3.status === 'running' && db3.coding_turn_id === p3.turn_id && r3.st === 'In Arbeit' && r3.turn === p3.turn_id && focused === p3.turn_id && (cmd3 === null || cmd3 === bugId),
    JSON.stringify({ l3: { type: l3.type, status: l3.status, code: l3.code, reason: l3.reason, keys: Object.keys(p3) }, prompt: p3.prompt, db3, r3, focused, cmd3 }));
  await P.screenshot({ path: shot('loop-auftrag-1280.png') });
  // LOOP-6 (Runde 2): „Bugs & Features“ → „An Coding Agent übergeben“ = api.shell.openSource(app, {tab:'reports', reportId}):
  // die Code-Ansicht der Ziel-App öffnet auf dem Reiter „Meldungen“ mit markierter Meldung; „In Arbeit“ aus coding_turn_id.
  const marks = () => P.$$eval(`${NWL} [data-v4-reports] [data-report-id].is-marked[aria-current="true"]`, (els) => els.map((e) => e.dataset.reportId));
  const side = () => P.$eval(`${NWL} [data-v4-source]`, (e) => e.dataset.side).catch(() => null);
  await P.click(`${NWL} [data-src-tab="files"]`).catch(() => {});
  const side6a = await side();
  // Pin 4b0d0ac: „Bugs & Features“ liest business_module_reports als Fremd-Collection (Eigentümer ctox) – nur mit der
  // Deklaration aus dem echten Manifest; ohne sie liefert der Kern eine leere Liste.
  const { _fixture_owners: _repOwners, ...repRoutes } = realManifestRoutes('reports');
  await H((extra) => window.__lab.kernel.sim.publishAppVersion('reports', '1.0.0', { title: 'Bugs & Features', layout: { icon_asset: 'icon.svg', category: 'Arbeit' }, styles: ['index.css'], manifestExtra: extra }), repRoutes);
  const rep = await open('reports', { x: 560, y: 300, w: 700, h: 470 });
  const RWL = '.shell-window[data-window-app="reports"]';
  await P.waitForSelector(`${RWL} [data-reports-list] [data-report-id="${wishId}"]`, { timeout: 8000 }).catch(() => {});
  const bugCard6 = await P.$eval(`${RWL} [data-reports-list] [data-report-id="${bugId}"]`, (e) => e.textContent.replace(/\s+/g, ' ').trim()).catch(() => '');
  const winsBefore6 = await P.$$eval('.shell-window', (e) => e.length);
  await P.click(`${RWL} [data-reports-list] [data-report-id="${wishId}"]`).catch(() => {});
  await P.waitForTimeout(150);
  await P.click(`${RWL} [data-head-delegate]`).catch(() => {});
  await P.waitForFunction((sel) => !!document.querySelector(sel), `${NWL}[data-v4-mode="code"] [data-v4-reports] [data-report-id="${wishId}"].is-marked`, { timeout: 4000 }).catch(() => {});
  const m6 = await marks();
  const side6 = await side();
  const tab6 = await P.$eval(`${NWL} [data-src-tab="reports"]`, (b) => b.getAttribute('aria-selected')).catch(() => null);
  const winsAfter6 = await P.$$eval('.shell-window', (e) => e.length);
  const front6 = await P.evaluate(() => document.querySelector('.shell-window.is-focused, .shell-window[data-focused="true"]')?.dataset.windowApp ?? null);
  await P.screenshot({ path: shot('loop-uebergabe-1280.png') });
  check('LOOP-6 „Bugs & Features“ → „An Coding Agent übergeben“: Code-Ansicht der Ziel-App (Notizen) wechselt vom Reiter „Dateien“ auf „Meldungen“, genau die gewählte Meldung #2 ist markiert, kein neues Fenster; die beauftragte Meldung #1 steht in der Liste „In Arbeit“ (coding_turn_id)',
    side6a === 'files' && side6 === 'reports' && tab6 === 'true' && m6.length === 1 && m6[0] === wishId && winsAfter6 === winsBefore6 && /In Arbeit/.test(bugCard6),
    JSON.stringify({ side6a, side6, tab6, m6, wishId, winsBefore6, winsAfter6, front6, bugCard6: bugCard6.slice(0, 160) }));
  await H((o) => window.__lab.frame.shellHandle().closeWindow(o), rep);
  // LOOP-7 (Runde 2): api.shell.openSource(appId, options) reicht {tab, reportId} an die Code-Ansicht durch; ungültige
  // Optionen → CtoxError invalid (nichts geöffnet, kein stilles Ignorieren).
  await P.click(`${NWL} [data-src-tab="files"]`).catch(() => {});
  const r7 = await H(([o, id]) => window.__lab.frame.shellHandle().win(o).host.instance.api.shell.openSource('notes', { reportId: id }), [tasks, bugId]);
  await P.waitForTimeout(250);
  const m7 = await marks();
  const side7 = await side();
  const r7f = await H((o) => window.__lab.frame.shellHandle().win(o).host.instance.api.shell.openSource('notes', { tab: 'files' }), tasks);
  await P.waitForTimeout(150);
  const side7f = await side();
  const bad7 = await H((o) => {
    const api = window.__lab.frame.shellHandle().win(o).host.instance.api;
    return [{ tab: 'x' }, { reportId: '' }, { foo: 1 }, 'reports'].map((opt) => { try { api.shell.openSource('notes', opt); return 'kein Fehler'; } catch (e) { return `${e.name}:${e.code}`; } });
  }, tasks);
  check('LOOP-7 api.shell.openSource(„notes“, {reportId}) aus Aufgaben: Reiter „Meldungen“, Meldung #1 markiert; {tab:"files"} schaltet auf Dateien; ungültige Optionen ({tab:"x"}, leere reportId, unbekanntes Feld, kein Objekt) → CtoxError invalid',
    r7 === true && side7 === 'reports' && m7.length === 1 && m7[0] === bugId && r7f === true && side7f === 'files' && bad7.every((b) => b === 'CtoxError:invalid'),
    JSON.stringify({ r7, side7, m7, r7f, side7f, bad7 }));
  await P.waitForSelector(`${NWL} .v4-agent__turn[data-state="done"]`, { timeout: 8000 }).catch(() => {});
  await P.waitForTimeout(200);
  // LOOP-4: Veröffentlichen schlägt „behebt Meldung #1“ vor; danach Meldung behoben mit Version, die andere bleibt offen.
  const activeBefore = await H(() => window.__lab.kernel.apps.get().find((a) => a.id === 'notes').active_ver);
  await P.click(`${NWL} [data-publish]`);
  await P.waitForSelector(`${NWL} [data-v4-sheet]`, { timeout: 3000 }).catch(() => {});
  const note4 = await P.$eval(`${NWL} [data-v4-sheet-input]`, (i) => i.value).catch(() => null);
  await P.screenshot({ path: shot('loop-versionsnotiz-1280.png') });
  await P.click(`${NWL} [data-v4-sheet-ok]`).catch(() => {});
  await P.waitForFunction((v) => window.__lab.kernel.apps.get().find((a) => a.id === 'notes').active_ver !== v, activeBefore, { timeout: 6000 }).catch(() => {});
  await P.waitForTimeout(400);
  const activeAfter = await H(() => window.__lab.kernel.apps.get().find((a) => a.id === 'notes').active_ver);
  const db4 = await reportsDb();
  const b4 = db4.find((r) => r.id === bugId) ?? {};
  const w4 = db4.find((r) => r.id === wishId) ?? {};
  const r4 = await rows();
  check('LOOP-4 Veröffentlichen mit verknüpfter Meldung: Versionsnotiz schlägt „behebt Meldung #1“ vor; nach dem Veröffentlichen steht #1 auf behoben (completed, fixed_in_ver = neue Version, Notiz), #2 bleibt offen, der Reiter zeigt nur noch #2',
    note4 === 'behebt Meldung #1' && activeAfter !== activeBefore && b4.status === 'completed' && b4.fixed_in_ver === activeAfter && b4.fixed_note === 'behebt Meldung #1' && w4.status === 'open'
      && r4.length === 1 && r4[0].id === wishId,
    JSON.stringify({ note4, activeBefore, activeAfter, b4, w4: w4.status, r4: r4.map((r) => r.id) }));
  // LOOP-5: Rückmeldung am Meldungs-Badge der meldenden Person, genau einmal; „Ansehen“ zeigt Notiz/Version, danach gelesen.
  const zoneFixed = (o) => H((o) => window.__lab.frame.shellHandle().win(o)?.host?.zone?.entries().filter((e) => /^shell\.report\.fixed:/.test(e.code)).map((e) => `${e.level}|${e.text}|${e.action?.label ?? ''}`) ?? [], o);
  const z5a = await zoneFixed(notes);
  await P.keyboard.press('Escape'); // zurück zur App
  await P.click(`${NWL} .v4-status-zone__badge`).catch(() => {});
  await P.waitForTimeout(120);
  await P.screenshot({ path: shot('loop-rueckmeldung-1280.png') });
  const viewBtn = await P.$$eval(`${NWL} .v4-status-zone__list:not([hidden]) .v4-status-zone__action:not([data-hide])`, (bs) => bs.map((b) => b.textContent));
  await P.click(`${NWL} .v4-status-zone__list:not([hidden]) .v4-status-zone__action:not([data-hide])`).catch(() => {});
  await P.waitForSelector(`${NWL} [data-v4-sheet]`, { timeout: 3000 }).catch(() => {});
  const sheet5 = await P.$eval(`${NWL} [data-v4-sheet]`, (s) => s.textContent).catch(() => '');
  await P.keyboard.press('Enter');
  await P.waitForTimeout(300);
  const seen5 = (await reportsDb()).find((r) => r.id === bugId)?.reporter_seen_at_ms ?? null;
  // Fenster schließen und neu öffnen: keine zweite Rückmeldung.
  const closed5 = await H((o) => window.__lab.frame.shellHandle().closeWindow(o), notes);
  await P.waitForTimeout(200);
  notes = await open('notes', { x: 20, y: 30, w: 860, h: 560 });
  await P.waitForTimeout(500);
  const z5b = await zoneFixed(notes);
  const zt = await zoneFixed(tasks);
  check('LOOP-5 Rückmeldung: am Meldungs-Badge des Notizen-Fensters einmal „Deine Meldung #1 ist behoben“ (info, Aktion „Ansehen“ → Blatt mit Version und Notiz); danach gelesen (reporter_seen_at_ms), nach neuem Öffnen kein zweites Mal; Aufgaben-Fenster bleibt still',
    z5a.length === 1 && z5a[0] === 'info|Deine Meldung #1 ist behoben|Ansehen' && viewBtn.includes('Ansehen') && /behebt Meldung #1/.test(sheet5) && sheet5.includes(activeAfter)
      && !!seen5 && closed5 && z5b.length === 0 && zt.length === 0,
    JSON.stringify({ closed5, z5a, viewBtn, sheet5, seen5, z5b, zt }));
  // LOOP-8 (Runde 2): Melder fremder Meldungen = Name aus der Nutzerliste der Instanz (business_users), sonst
  // „Teammitglied“ – nie eine ID.
  const now8 = Date.now();
  const seed8 = await H(async ([now]) => {
    const api = window.__lab.kernel.appApi('ctox', '1');
    const row = (id, who, title, extra = {}) => ({ id, report_id: id, module_id: 'notes', kind: 'bug', title, summary: title, status: 'open', reporter_id: who, created_at_ms: now, updated_at_ms: now, ...extra });
    const out = [];
    for (const [c, d] of [
      ['business_users', { id: 'u-kim', user_id: 'kim', display_name: 'Kim Berger', role: 'user', active: true, updated_at_ms: now }],
      ['business_module_reports', row('rep-kim', 'kim', 'Fremde Meldung mit Namen')],
      ['business_module_reports', row('rep-ghost', 'ghost-7f3a', 'Fremde Meldung ohne Nutzer')],
      ['business_module_reports', row('rep-self', 'u-lena', 'Fremde Meldung mit ID als Name', { reporter_display_name: 'u-lena' })],
    ]) { try { const h = api.put(c, d); await (h?.local ?? h); out.push('ok'); } catch (e) { out.push(String(e?.code || e)); } }
    return out;
  }, [now8]);
  await H((o) => window.__lab.frame.shellHandle().win(o).host.instance.api.shell.openSource('notes', { tab: 'reports' }), notes);
  await P.waitForSelector(`${NWL} [data-v4-reports] [data-report-id="rep-ghost"]`, { timeout: 6000 }).catch(() => {});
  await P.waitForTimeout(300);
  const r8 = Object.fromEntries((await rows()).map((r) => [r.id, r.by]));
  check('LOOP-8 Melder-Name: fremde Meldung zeigt den Namen aus der Nutzerliste („Kim Berger“), ohne Nutzereintrag „Teammitglied“, auch wenn nur eine ID als Name gespeichert ist; eigene Meldung „Du“; nirgends eine Melder-ID',
    seed8.every((x) => x === 'ok') && r8['rep-kim'] === 'Kim Berger' && r8['rep-ghost'] === 'Teammitglied' && r8['rep-self'] === 'Teammitglied' && r8[wishId] === 'Du'
      && !Object.values(r8).some((b) => /kim$|ghost|u-lena/.test(b)),
    JSON.stringify({ seed8, r8 }));
  await P.screenshot({ path: shot('loop-melder-1280.png') });
  check('LOOP ohne Seitenfehler', loopErrors().length === 0, loopErrors().join(' | '));
  await ctx.close();
}

// ================= FR6c: EINE Leiste (Reiter oben, Shell-V2) mit der FR6b-Gruppierung; Desktop im Launcher-Fuß =================
// Eigene Seite. FR6C_ONLY=1 lässt nur diesen Block laufen. Mutationsbeleg je Punkt: dieselbe Messung läuft mit einer
// per page.route mutierten Rahmen-Datei; der zugehörige Punkt muss dort scheitern.
const FR6C_MUTATIONS = {
  group: ['taskbar.js', "if (!key) return null;", 'return null;'],
  count: ['topbar.js', 'count.hidden = n < 2;', 'count.hidden = true;'],
  pin: ['taskbar.js', 'entryFor(id).pins.push(id);', "entryFor('pin:' + id).pins.push(id);"],
  active: ['topbar.js', "let priority = items.findIndex((i) => i.dataset.running === 'focused');\n    if (priority < 0) priority = items.indexOf(focused);", 'let priority = -1;'],
  desktop: ['launcher.js', "deskBtn.addEventListener('click', () => { close({ restore: false }); onDesktop?.(); });", "deskBtn.addEventListener('click', () => { close({ restore: false }); });"],
  // FR6d: Startmenü (Klick daneben, Esc), Desktop-Fläche, Crew-Leiste
  menu: ['launcher.js', "document.addEventListener('mousedown', onOutside, true);", ''],
  menuEsc: ['launcher.js', "if (event.key !== 'Escape') return;", 'return;'],
  surface: ['desktop.js', 'host.insertBefore(el, before);', ''],
  crew: ['crew-bar.js', 'if (slot && membersEl.contains(slot)) onMember?.(slot.dataset.v4Crew, { anchor: slot, event });', 'if (slot && membersEl.contains(slot)) {}'],
  gear: ['topbar.js', '@media (max-width: 559.98px)', '@media (max-width: 1px)'],
};
// FR6d-G0: dieselben DOM-Teile im v4-Rahmen (shell-v2-desktop.css) und im app.js-Pfad (Originalquellen) pixelweise.
async function fr6dG0(P, ctx) {
  const chat = await chatCss();
  const theme = await P.evaluate(() => document.documentElement.dataset.theme);
  const ref = async (body, bg, clip) => {
    const R = await ctx.newPage();
    await R.setViewportSize({ width: 1280, height: 800 });
    await R.goto(`${base}/v4/frame/lab/index.html?fr6d-g0=1`).catch(() => {});
    await R.setContent(`<!doctype html><html lang="de" data-theme="${theme}" data-shell-style="ctox"><head><meta charset="utf-8">
      <link rel="stylesheet" href="${base}/ui-contract/v1/workjet-ui-contract.css"><link rel="stylesheet" href="${base}/app.css"><link rel="stylesheet" href="${base}/shared/base.css">
      <link rel="stylesheet" href="${base}/mobile-host.css"><link rel="stylesheet" href="${base}/modules/desktop/index.css"><style>${chat}</style>
      <style>html,body{background:${bg} !important}</style></head><body data-module-shell="full">${body}</body></html>`, { waitUntil: 'load' });
    await R.mouse.move(1270, 790);
    await R.evaluate(() => Promise.all([...document.images].map((i) => (i.complete ? null : new Promise((r) => { i.onload = i.onerror = r; })))));
    await R.waitForTimeout(300);
    const png = await R.screenshot({ clip });
    await R.close();
    return png;
  };
  const still = async () => { await P.mouse.move(1270, 790); await P.evaluate(() => document.activeElement?.blur?.()); await P.waitForTimeout(300); };
  const out = {};
  // Desktop (Fenster minimiert, Menü zu): Symbolzeile; Crew-Leiste dafür ausgeblendet.
  await P.evaluate(() => { document.querySelector('.v4f-crew').style.visibility = 'hidden'; });
  await still();
  const dClip = { x: 0, y: 48, width: 1280, height: 200 };
  const dV4 = await P.screenshot({ clip: dClip });
  const dHtml = await P.evaluate(() => document.querySelector('[data-v4-desktop-v2]').innerHTML);
  const dBg = await P.evaluate(() => getComputedStyle(document.querySelector('[data-v4-frame]')).backgroundColor);
  const dRef = await ref(`<div data-app="desktop" style="position:fixed;left:0;top:48px;width:1280px;height:752px">${dHtml}</div>`, dBg, dClip);
  out.desktop = (await pxCompare(P, dV4, dRef, { all: [0, 0, 1280, 200] })).all;
  await P.evaluate(() => { document.querySelector('.v4f-crew').style.visibility = ''; });
  // Crew-Leiste: Hintergrund = Desktop-Fläche an der Stelle.
  await still();
  const cr = await P.evaluate(() => { const r = document.querySelector('[data-v4-crew-bar] .ctox-chat-dock').getBoundingClientRect(); return { x: Math.floor(r.left) - 2, y: Math.floor(r.top) - 2, width: Math.ceil(r.width) + 4, height: Math.ceil(r.height) + 4 }; });
  await P.evaluate(() => { document.querySelector('[data-v4-desktop]').style.visibility = 'hidden'; });
  const cV4 = await P.screenshot({ clip: cr });
  const cHtml = await P.evaluate(() => document.querySelector('.v4f-crew').innerHTML);
  const cRef = await ref(cHtml, dBg, cr);
  out.crew = (await pxCompare(P, cV4, cRef, { all: [0, 0, cr.width, cr.height] })).all;
  fs.writeFileSync(shot('fr6d-g0-crew-v4.png'), cV4); fs.writeFileSync(shot('fr6d-g0-crew-ref.png'), cRef);
  // Startmenü offen, Desktop und Crew-Leiste aus (das Glas-Panel lässt den Grund durchscheinen).
  await P.evaluate(() => { document.querySelector('.v4f-crew').style.visibility = 'hidden'; });
  await P.click('[data-shell-start]');
  // „Neue App“ hat keine Entsprechung in der Shell-V2 (v4-Zusatz, frame.css): für den Vergleich ausgeblendet.
  const acts = await P.evaluate(() => { const a = document.querySelector('[data-v4-launcher-actions]'); const was = a.hidden; a.hidden = true; return was; });
  await still();
  const sr = await P.evaluate(() => { const r = document.querySelector('[data-v4-launcher]').getBoundingClientRect(); return { x: Math.floor(r.left), y: Math.floor(r.top), width: Math.ceil(r.width), height: Math.ceil(r.height) }; });
  const sV4 = await P.screenshot({ clip: sr });
  const sHtml = await P.evaluate(() => document.querySelector('[data-v4-launcher]').outerHTML);
  const sRef = await ref(sHtml, dBg, sr);
  out.start = (await pxCompare(P, sV4, sRef, { all: [0, 0, sr.width, sr.height] })).all;
  fs.writeFileSync(shot('fr6d-g0-start-v4.png'), sV4); fs.writeFileSync(shot('fr6d-g0-start-ref.png'), sRef);
  fs.writeFileSync(shot('fr6d-g0-desktop-v4.png'), dV4); fs.writeFileSync(shot('fr6d-g0-desktop-ref.png'), dRef);
  // Mutation: Kopf des Startmenüs mit 40 px Innenabstand muss abweichen.
  await P.evaluate(() => document.querySelector('[data-v4-launcher] .start-menu-body').style.setProperty('padding-left', '40px', 'important'));
  await still();
  const mV4 = await P.screenshot({ clip: sr });
  out.startMut = (await pxCompare(P, mV4, sRef, { all: [0, 0, sr.width, sr.height] })).all;
  await P.evaluate(() => document.querySelector('[data-v4-launcher] .start-menu-body').style.removeProperty('padding-left'));
  await P.evaluate((was) => { document.querySelector('[data-v4-launcher-actions]').hidden = was; }, acts);
  await P.keyboard.press('Escape');
  await P.evaluate(() => { document.querySelector('.v4f-crew').style.visibility = ''; document.querySelector('[data-v4-desktop]').style.visibility = ''; });
  return out;
}
async function fr6cRun(mut = null, { shots: takeShots = false } = {}) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const P = await ctx.newPage();
  const perr = [];
  P.on('pageerror', (e) => perr.push(String(e)));
  P.on('console', (m) => { if (m.type() === 'error' && !/404|Failed to load resource/.test(m.text())) perr.push(m.text()); });
  if (mut) {
    const [file, from, to] = FR6C_MUTATIONS[mut];
    await P.route(new RegExp(`/v4/frame/${file.replace('.', '\\.')}(\\?|$)`), async (route) => {
      const res = await route.fetch();
      const body = await res.text();
      if (!body.includes(from)) throw new Error(`FR6c-Mutation ${mut}: Stelle nicht gefunden`);
      await route.fulfill({ response: res, body: body.replace(from, to) });
    });
  }
  const CREW = [['crew-anna', 'Anna', '#c2410c'], ['crew-ben', 'Ben', '#0f766e'], ['crew-cleo', 'Cleo', '#7c3aed']];
  await P.route(/\/~a\/crew-[a-z]+@/, (route) => {
    const id = /\/~a\/(crew-[a-z]+)@/.exec(route.request().url())[1];
    const name = CREW.find((c) => c[0] === id)?.[1] || id;
    route.fulfill({ status: 200, contentType: 'text/javascript', body: `export function mount(root, api) {
      root.innerHTML = '<div style="font:13px system-ui;padding:12px 14px"><p>Crew-Chat</p></div>';
      api.shell?.setTitle?.('Chat ${name}');
      return { unmount() { root.replaceChildren(); }, snapshot() { return {}; } };
    }` });
  });
  await P.goto(`${base}/v4/frame/lab/${STUB ? `stub-lab.html?inst=fr6c-${Date.now().toString(36)}-${mut || 'x'}` : 'index.html'}`);
  await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  await P.evaluate(async ([stub, crew]) => {
    const add = (id, title, layout, ver = '1.0.0') => (stub
      ? window.__lab.kernel.sim.publishAppVersion(id, ver, { title, layout, styles: [] })
      : window.__lab.kernel.sim.addApp({ id, title, active_ver: ver, activated_ver: ver, state: 'activated', target_ver: null, error: null },
        { app_id: id, app_ver: ver, entry: 'index.js', layout }));
    for (const [id, name, color] of crew) {
      const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" fill="${color}"/><circle cx="32" cy="26" r="11" fill="#fff"/><path d="M12 58c3-13 12-19 20-19s17 6 20 19" fill="#fff"/></svg>`;
      await add(id, `Crew ${name}`, { category: 'Crew', icon_svg: svg, default_width: 420, default_height: 320, presentation: { taskbar_group: 'crew' } });
    }
  }, [STUB, CREW]);
  await P.waitForSelector('[data-v4-app="crew-cleo"]', { state: 'attached' });
  const open = (id, g) => P.evaluate(async ([id, g]) => { const h = window.__lab.frame.shellHandle(); const o = h.openWindow(id, { geometry: g }); await h.win(o).host.ready; return o; }, [id, g]);
  await open('notes', { x: 30, y: 30, w: 460, h: 300 });
  await open('tasks', { x: 520, y: 30, w: 460, h: 300 });
  await open('ledger', { x: 260, y: 260, w: 460, h: 300 });
  for (let i = 0; i < 6; i++) await open(CREW[i % 3][0], { x: 60 + i * 50, y: 90 + i * 30, w: 380, h: 260 });
  // angeheftet: tasks (hat Fenster) und crew-anna (Gruppe) – müssen mit ihren Reitern verschmelzen
  for (const id of ['tasks', 'crew-anna']) await P.$eval(`[data-v4-app="${id}"] [data-v4-pin]`, (b) => b.click());
  await P.waitForTimeout(300);
  const bar = () => P.evaluate(() => {
    const el = document.querySelector('[data-module-tabs]');
    const tr = el.getBoundingClientRect();
    const all = [...el.querySelectorAll('.module-tab[data-v4-task-group]')];
    const more = el.querySelector(':scope > details.module-overflow');
    return {
      entries: all.map((n) => ({ g: n.dataset.v4TaskGroup, label: n.querySelector('.module-tab-label').textContent, count: getComputedStyle(n.querySelector('[data-v4-count]')).display !== 'none' ? Number(n.querySelector('[data-v4-count]').textContent) : 0, pin: n.dataset.v4Pin || '', run: n.dataset.running || '', active: n.getAttribute('aria-current') === 'page', hidden: !!n.closest('details') })),
      more: more ? more.querySelector('summary').textContent : '',
      fits: all.filter((n) => !n.closest('details')).every((n) => { const r = n.getBoundingClientRect(); return r.left >= tr.left - 0.5 && r.right <= tr.right + 0.5 && r.width > 0; })
        && document.querySelector('[data-v4-topbar] .topbar').scrollWidth <= document.querySelector('[data-v4-topbar] .topbar').clientWidth + 1 && document.documentElement.scrollWidth <= innerWidth,
      bottomBar: !!document.querySelector('[data-v4-taskbar], .v4f-bar'),
    };
  });
  const out = { perr };
  out.wide = await bar();
  if (takeShots) await P.screenshot({ path: shot('fr6c-tabs-1280.png') });
  // aktiv = Crew (letzter Reiter); schmal muss er sichtbar bleiben
  await P.click('[data-v4-task-group="crew"]', { timeout: 3000 }).catch(() => {}); // fehlt nur unter Mutation „group“
  await P.waitForTimeout(100);
  await P.evaluate(() => { document.querySelector('[data-module-tabs]').style.maxWidth = '200px'; });
  await P.waitForTimeout(250);
  out.narrow = await bar();
  await P.evaluate(() => { document.querySelector('[data-module-tabs]').style.maxWidth = ''; });
  await P.waitForTimeout(250);
  // Desktop im Launcher-Fuß
  const vis = () => P.evaluate(() => window.__lab.frame.shellHandle().windows().filter((r) => r.handle.element.style.display !== 'none' && !r.handle.element.classList.contains('is-minimized')).length);
  out.visBefore = await vis();
  out.foot = await P.evaluate(() => { const f = document.querySelector('[data-v4-launcher] [data-v4-launcher-foot]'); return f ? [...f.children].filter((b) => getComputedStyle(b).display !== 'none').map((b) => b.textContent.trim()) : []; });
  // FR6d-menu: Startmenü = Popup aus dem Raster-Knopf; Esc und Klick daneben schließen es.
  const menuOpen = () => P.$eval('[data-v4-launcher]', (e) => e.classList.contains('is-active'));
  out.menu = { closed0: !(await menuOpen()) };
  await P.click('[data-shell-start]'); await P.waitForTimeout(120);
  out.menu.open1 = await menuOpen();
  out.menu.searchFocus = await P.evaluate(() => document.activeElement?.matches('[data-v4-search]') || false);
  out.menu.panel = await P.evaluate(() => { const r = document.querySelector('[data-v4-launcher]').getBoundingClientRect(); return [Math.round(r.left), Math.round(r.top), Math.round(r.width)]; });
  if (takeShots) await P.screenshot({ path: shot('fr6d-start-1280.png') });
  await P.keyboard.press('Escape'); await P.waitForTimeout(120);
  out.menu.esc = !(await menuOpen());
  await P.click('[data-shell-start]'); await P.waitForTimeout(120);
  out.menu.open2 = await menuOpen();
  await P.mouse.click(1100, 760); await P.waitForTimeout(120);
  out.menu.outside = !(await menuOpen());
  if (await menuOpen()) await P.click('[data-shell-start]');
  await P.click('[data-shell-start]'); await P.waitForTimeout(120);
  await P.click('[data-v4-launcher-desktop]');
  await P.waitForTimeout(600);
  out.visMid = await vis();
  out.menuAfterDesk = !(await menuOpen());
  // FR6d-surface: hinter den minimierten Fenstern liegt der Desktop der Shell-V2 (Symbole, Klick startet).
  out.surface = await P.evaluate(() => {
    const icons = [...document.querySelectorAll('[data-v4-desktop] .desktop-icon')];
    const first = icons[0]?.getBoundingClientRect();
    const hit = first ? document.elementFromPoint(first.left + first.width / 2, first.top + 30) : null;
    return { n: icons.length, hit: !!hit?.closest('[data-v4-desktop] .desktop-icon'), pos: first ? [Math.round(first.left), Math.round(first.top), Math.round(first.width), Math.round(first.height)] : null };
  });
  if (takeShots) {
    await P.screenshot({ path: shot('fr6c-launcher-desktop-1280.png') });
    out.g0 = await fr6dG0(P, ctx);
  }
  await P.click('[data-shell-start]'); await P.waitForTimeout(120);
  await P.click('[data-v4-launcher-desktop]');
  await P.waitForTimeout(600);
  out.visAfter = await vis();
  // FR6d-crew: Crew-Leiste unten links, Klick auf ein Mitglied holt dessen Chat nach vorn (Reiter „Crew“ aktiv).
  const front = () => P.evaluate(() => { const ws = window.__lab.frame.shellHandle().windows().filter((r) => r.handle.element.style.display !== 'none' && !r.handle.element.classList.contains('is-minimized')); return ws.sort((a, b) => Number(b.handle.element.style.zIndex) - Number(a.handle.element.style.zIndex))[0]?.appId || null; });
  out.crew = await P.evaluate(() => {
    const bar = document.querySelector('[data-v4-crew-bar]');
    const r = bar?.querySelector('.ctox-chat-dock')?.getBoundingClientRect();
    return { members: [...document.querySelectorAll('[data-v4-crew]')].map((e) => e.dataset.v4Crew), label: bar?.querySelector('.ctox-chat-fab-label')?.textContent, nav: bar?.querySelectorAll('.ctox-chat-date-pill .ctox-date-nav-btn, .ctox-chat-date-pill .ctox-date-picker-trigger').length || 0, rect: r ? [Math.round(r.left), Math.round(r.bottom)] : null, visible: !!r && r.width > 0 };
  });
  out.crew.front0 = await front();
  await P.click('[data-v4-crew="crew-ben"]').catch(() => {});
  await P.waitForTimeout(300);
  out.crew.front1 = await front();
  out.crew.activeTab = await P.evaluate(() => document.querySelector('[data-module-tabs] .module-tab[aria-current="page"]')?.dataset.v4TaskGroup || null);
  const day0 = await P.$eval('[data-v4-crew-bar]', (b) => b.dataset.v4CrewDay);
  await P.click('[data-v4-crew-prev]').catch(() => {});
  const day1 = await P.$eval('[data-v4-crew-bar]', (b) => b.dataset.v4CrewDay);
  await P.click('[data-v4-crew-next]').catch(() => {});
  await P.click('[data-v4-crew-next]').catch(() => {}); // nie über heute hinaus
  const day2 = await P.$eval('[data-v4-crew-bar]', (b) => b.dataset.v4CrewDay);
  out.crew.days = [day0, day1, day2];
  if (takeShots) await P.screenshot({ path: shot('fr6d-crew-1280.png') });
  // 420: aktiver Reiter sichtbar, Zahnrad weg, Einstellungen über das Konto
  await P.setViewportSize({ width: 420, height: 800 });
  await P.waitForTimeout(400);
  out.n420 = await bar();
  out.gear420 = await P.$eval('[data-v4-settings-button]', (b) => getComputedStyle(b).display);
  if (takeShots) await P.screenshot({ path: shot('fr6c-tabs-420.png') });
  await P.click('[data-open-account]'); await P.waitForTimeout(150);
  out.accFirst = await P.evaluate(() => document.querySelector('[data-v4-account-panel] .account-drawer')?.firstElementChild?.textContent.trim() || '');
  await P.click('[data-v4-account-settings]'); await P.waitForTimeout(150);
  out.settings420 = await P.evaluate(() => { const p = document.querySelector('[data-v4-settings-panel]'); const r = p.getBoundingClientRect(); return !p.hidden && r.width > 0 && r.right <= innerWidth + 1; });
  await P.keyboard.press('Escape');
  if (takeShots) {
    await P.click('[data-shell-start]'); await P.waitForTimeout(600);
    await P.screenshot({ path: shot('fr6c-launcher-420.png') });
    await P.click('[data-shell-start]'); await P.waitForTimeout(400);
  }
  await P.setViewportSize({ width: 1280, height: 800 });
  await ctx.close();
  return out;
}
function fr6cVerdicts(o) {
  const crew = o.wide.entries.find((e) => e.g === 'crew');
  const tasks = o.wide.entries.find((e) => e.g === 'tasks');
  const nCrew = o.narrow.entries.find((e) => e.g === 'crew');
  const a420 = o.n420.entries.find((e) => e.active);
  return {
    group: !o.wide.bottomBar && o.wide.entries.length === 4 && JSON.stringify(o.wide.entries.map((e) => e.g)) === JSON.stringify(['tasks', 'crew', 'notes', 'ledger']) && crew?.label === 'Crew',
    count: crew?.count === 6 && o.wide.entries.filter((e) => e.g !== 'crew').every((e) => e.count === 0),
    pin: tasks?.pin === 'tasks' && !!tasks?.run && crew?.pin === 'crew-anna' && o.wide.entries.filter((e) => e.g === 'tasks' || e.g === 'crew').length === 2,
    active: /^\+\d+$/.test(o.narrow.more) && nCrew?.active && !nCrew.hidden && o.narrow.fits && o.wide.fits && !o.wide.more,
    desktop: JSON.stringify(o.foot) === JSON.stringify(['Desktop', 'Einstellungen']) && o.visBefore === 9 && o.visMid === 0 && o.visAfter === 9 && o.menuAfterDesk,
    menu: o.menu.closed0 && o.menu.open1 && o.menu.searchFocus && o.menu.esc && o.menu.open2 && o.menu.outside && o.menu.panel[0] === 16 && o.menu.panel[1] === 48 && o.menu.panel[2] === 360,
    menuEsc: o.menu.esc,
    surface: o.surface.n === 6 && o.surface.hit && JSON.stringify(o.surface.pos) === JSON.stringify([24, 72, 112, 136]),
    crew: JSON.stringify(o.crew.members) === JSON.stringify(['crew-anna', 'crew-ben', 'crew-cleo']) && o.crew.label === 'Crew' && o.crew.nav === 3 && o.crew.visible && o.crew.rect[0] <= 24 && o.crew.rect[1] >= 760
      && o.crew.front0 !== 'crew-ben' && o.crew.front1 === 'crew-ben' && o.crew.activeTab === 'crew' && o.crew.days[1] < o.crew.days[0] && o.crew.days[2] === o.crew.days[0],
    gear: o.gear420 === 'none' && o.accFirst === 'Einstellungen' && o.settings420 && !!a420 && !a420.hidden && o.n420.fits,
  };
}
async function fr6c() {
  const base0 = await fr6cRun(null, { shots: true });
  const v = fr6cVerdicts(base0);
  const names = {
    group: 'gruppiert: EIN Reiter je App, Crew-Chats unter „Crew“, keine Taskleiste unten',
    count: 'Zahl ab zwei Fenstern (Crew 6), Einzelfenster ohne Zahl',
    pin: 'Pin verschmolzen: angeheftete App und angeheftetes Crew-Mitglied = derselbe Reiter',
    active: 'Überlauf „+N“ gruppiert, aktives Fenster (Crew, letzter Reiter) bleibt sichtbar, Leiste wächst nie über ihre Breite',
    desktop: 'Desktop im Startmenü-Fuß (links, „Einstellungen“ daneben): schließt das Menü, 9 → 0 sichtbare Fenster, zweiter Klick → 9',
    menu: 'FR6d Startmenü aus dem Raster-Knopf (16/48, 360 px breit wie Shell-V2 auf welsch), Suche fokussiert; Esc schließt, Klick daneben schließt',
    menuEsc: 'FR6d Startmenü: Esc schließt',
    surface: 'FR6d Desktop als eigene Fläche wie Shell-V2: 6 App-Symbole im Raster 120 × 140 ab 24/24 (112 × 136), frei klickbar hinter den minimierten Fenstern',
    crew: 'FR6d Crew-Leiste unten links: Pille „Crew“ mit den 3 Crew-Mitgliedern, Kalender-Navigator ‹ 📅 ›; Klick auf Ben holt dessen Chat nach vorn (Reiter „Crew“ aktiv); Tag zurück/vor, nie über heute',
    gear: '420 px: Zahnrad weg, aktiver Reiter sichtbar, „Einstellungen“ oberster Eintrag im Konto-Blatt und öffnet das Blatt',
  };
  for (const k of Object.keys(names)) check(`FR6c-${k} ${names[k]}`, v[k], JSON.stringify(k === 'desktop' ? { foot: base0.foot, b: base0.visBefore, m: base0.visMid, a: base0.visAfter, closed: base0.menuAfterDesk } : k.startsWith('menu') ? base0.menu : k === 'surface' ? base0.surface : k === 'crew' ? base0.crew : k === 'gear' ? { gear: base0.gear420, acc: base0.accFirst, s: base0.settings420, n420: base0.n420 } : k === 'active' ? base0.narrow : base0.wide));
  check('FR6c ohne Seitenfehler', base0.perr.length === 0, base0.perr.join(' | '));
  const g = base0.g0 || {};
  check('FR6d-G0 Startmenü, Desktop und Crew-Leiste = app.css-Pfad (ui-contract + app.css + base.css + mobile-host.css + modules/desktop/index.css + business-chat-Stil) mit demselben DOM (ohne den v4-Zusatz „Neue App“), hell 1280 (≥ 99,9 % Pixel)',
    ['start', 'desktop', 'crew'].every((k) => g[k] >= 0.999), JSON.stringify(g));
  check('FR6d-G0 Mutationsprobe: verschobene Startmenü-Einträge (Innenabstand Rumpf 40 px) weichen ab (< 98 %)', g.startMut < 0.98, JSON.stringify(g));
  for (const k of Object.keys(FR6C_MUTATIONS)) {
    const m = await fr6cRun(k).catch((e) => ({ error: String(e) }));
    const mv = m.error ? null : fr6cVerdicts(m);
    check(`FR6c-${k} Mutationsprobe (${FR6C_MUTATIONS[k][0]}): Punkt „${k}“ wird rot`, mv && !mv[k],
      JSON.stringify(mv || m));
  }
}

// ================= ZONE2 (K24, Befund T3-1): Zonen-Garantie für JEDE App, auch ohne Kit =================
// Eigene Seite. ZONE2_ONLY=1 lässt nur diesen Block laufen. Lab-App „bare“ kennt das Kit nicht (Knöpfe oben links, oben
// rechts und absolut oben rechts, wie die K24-Probe). Gegenprobe je Fall: die Garantie per Stil neutralisiert → rot.
// Code-Ansicht (nur stub, die Attrappe hat kein kernel.source): Kopf frei im Rahmen-Wirt bei 1280 und 900, und mit einem
// Wirt OHNE Zonen-Abstand (wie der K24-Einstieg kernel-k1 openSource) hält die Ansicht die Zone selbst frei.
async function zone2() {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const P = await ctx.newPage();
  const perr = [];
  P.on('pageerror', (e) => perr.push(String(e)));
  P.on('console', (m) => { if (m.type() === 'error' && !/404|Failed to load resource/.test(m.text())) perr.push(m.text()); });
  // Sichtbare Lage der App-Wurzel je Bild (computed top/right): zählt jeden sichtbaren Wechsel (Flackern).
  await P.addInitScript(() => {
    window.__z2Samples = [];
    const tick = () => {
      const r = document.querySelector('.shell-window[data-window-app="bare"] .shell-window-content > .v4-app-root');
      if (r) { const cs = getComputedStyle(r); const v = `${cs.top}|${cs.right}`; const s = window.__z2Samples; if (s[s.length - 1] !== v) s.push(v); }
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });
  await P.goto(`${base}/v4/frame/lab/${STUB ? `stub-lab.html?inst=zone2-${Date.now().toString(36)}` : 'index.html'}`);
  await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  await P.evaluate(async (stub) => {
    const add = (id, title, layout, ver = '1.0.0') => (stub
      ? window.__lab.kernel.sim.publishAppVersion(id, ver, { title, layout, styles: [] })
      : window.__lab.kernel.sim.addApp({ id, title, active_ver: ver, activated_ver: ver, state: 'activated', target_ver: null, error: null },
        { app_id: id, app_ver: ver, entry: 'index.js', layout }));
    await add('bare', 'Fremde App', { category: 'Arbeit', default_width: 760, default_height: 520 });
    await add('studio', 'Black Hole Studio', { category: 'Arbeit', default_width: 900, default_height: 520 });
    await add('crm', 'Kontakte', { category: 'Vertrieb', default_width: 900, default_height: 520 }, '2.0.0');
  }, STUB);
  const open = (id, g) => P.evaluate(async ([id, g]) => { const h = window.__lab.frame.shellHandle(); const o = h.openWindow(id, { geometry: g }); await h.win(o).host.ready; return o; }, [id, g]);
  const kit = {};
  // Lab-App notes kennt das Kit nicht (Eingabe oben links unter dem Icon) → die Garantie greift auch dort.
  const notes = await open('notes', { x: 20, y: 20, w: 560, h: 360 });
  kit.studio = await open('studio', { x: 600, y: 20, w: 640, h: 360 });
  kit.crm = await open('crm', { x: 600, y: 400, w: 640, h: 360 });
  const bare = await open('bare', { x: 220, y: 140, w: 760, h: 520 });
  await P.waitForTimeout(400);
  const W = '.shell-window[data-window-app="bare"]';
  // Treffertest am Mittelpunkt + echter Mausklick (Zähler der App) je Knopf.
  const probe = () => P.$eval(W, (w) => {
    const root = w.querySelector('.shell-window-content > .v4-app-root');
    return { guard: root?.getAttribute('data-v4-zone-guard') ?? null, top: getComputedStyle(root).top,
      btns: Object.fromEntries([...root.querySelectorAll('[data-bare]')].map((b) => { const r = b.getBoundingClientRect(); const x = r.left + r.width / 2; const y = r.top + r.height / 2; const hit = document.elementFromPoint(x, y); return [b.dataset.bare, { x: Math.round(x), y: Math.round(y), free: !!hit && b.contains(hit), by: hit && !b.contains(hit) ? `${hit.tagName.toLowerCase()}.${String(hit.className).split(' ')[0]}` : null }]; })) };
  });
  const clickAll = async (pr) => {
    await P.evaluate(() => { globalThis.__bareClicks = {}; });
    for (const b of Object.values(pr.btns)) await P.mouse.click(b.x, b.y);
    await P.waitForTimeout(80);
    return P.evaluate(() => ({ ...globalThis.__bareClicks }));
  };
  const z1 = await probe();
  const mountSamples = await P.evaluate(() => window.__z2Samples.slice());
  const c1 = await clickAll(z1);
  await P.screenshot({ path: shot('zone2-bare-1280.png') });
  check('ZONE2 App ohne Kit: Knöpfe oben links, oben rechts und absolut oben rechts liegen nach dem Mount frei (elementFromPoint am Mittelpunkt = Knopf) und reagieren auf die echte Maus',
    z1.guard === 'top' && ['left', 'right', 'abs', 'tick'].every((k) => z1.btns[k]?.free && c1[k] === 1), JSON.stringify({ z1, c1 }));
  const kitGuard = await P.evaluate((ids) => Object.fromEntries(Object.entries(ids).map(([k, o]) => [k, window.__lab.frame.shellHandle().win(o).handle.element.querySelector('.v4-app-root')?.getAttribute('data-v4-zone-guard') ?? null])), kit);
  const notesGuard = await P.evaluate((o) => window.__lab.frame.shellHandle().win(o).handle.element.querySelector('.v4-app-root')?.getAttribute('data-v4-zone-guard') ?? null, notes);
  check('ZONE2 Kit-App unberührt (studio mit Kit-Kopf: kein Attribut, Optik bleibt); Apps ohne Kit mit Eingabe unter dem Icon (notes oben links, crm unter der v2-Kopfzeile bei y 53): Garantie greift',
    kitGuard.studio === null && kitGuard.crm === 'top' && notesGuard === 'top', JSON.stringify({ kitGuard, notesGuard }));
  // Gegenprobe: Garantie neutralisiert → „Agent“ oben rechts liegt unter dem Knopfcluster, „Links“ unter dem Icon.
  await P.addStyleTag({ content: '.v4f .shell-window-content > .v4-app-root[data-v4-zone-guard] { top: 0 !important; right: 0 !important; }' });
  await P.waitForTimeout(80);
  const zm = await probe();
  // Nur Treffertest: ein Mausklick träfe hier Icon bzw. Schließen-Knopf des Fensters.
  check('ZONE2 Gegenprobe: ohne die Garantie liegen „Links“ (unter dem Icon) und der Knopf oben rechts (unter dem Knopfcluster) verdeckt',
    !zm.btns.left.free && !zm.btns.abs.free && /shell-window/.test(`${zm.btns.left.by} ${zm.btns.abs.by}`), JSON.stringify({ zm }));
  await P.evaluate((o) => { document.querySelectorAll('style').forEach((s) => { if (s.textContent.includes('data-v4-zone-guard] { top: 0')) s.remove(); }); window.__lab.frame.shellHandle().win(o).zoneGuard.measure(); }, bare);
  await P.waitForTimeout(100);
  await P.evaluate(() => { window.__z2Samples.length = 0; });
  // Kein Flackern: Mount → genau ein sichtbarer Wechsel (0 → Zonenhöhe); Maximieren, Wiederherstellen und DOM-Änderungen
  // der App → kein weiterer Wechsel.
  await P.evaluate(async (o) => { const h = window.__lab.frame.shellHandle().win(o).handle; h.maximize(); await new Promise((r) => setTimeout(r, 300)); h.restore(); await new Promise((r) => setTimeout(r, 300));
    const ul = document.querySelector('.shell-window[data-window-app="bare"] [data-bare-list]'); for (let i = 0; i < 20; i++) { const li = document.createElement('li'); li.textContent = `Zeile ${i + 2}`; ul.append(li); await new Promise((r) => requestAnimationFrame(r)); } }, bare);
  await P.waitForTimeout(200);
  const samples = await P.evaluate(() => window.__z2Samples.slice());
  const z2 = await probe();
  check('ZONE2 kein Flackern: genau ein sichtbarer Wechsel beim Mount (0 → Zonenhöhe), keiner bei Maximieren, Wiederherstellen und DOM-Änderung; Garantie bleibt',
    mountSamples.length === 2 && mountSamples[1] === '74px|0px' && samples.length === 1 && samples[0] === '74px|0px' && z2.guard === 'top' && z2.btns.right.free, JSON.stringify({ mountSamples, samples, guard: z2.guard }));
  // 900 px: dieselbe Garantie.
  await P.setViewportSize({ width: 900, height: 800 });
  await P.evaluate((o) => window.__lab.frame.shellHandle().win(o).handle.element.style.left = '40px', bare);
  await P.waitForTimeout(300);
  const z9 = await probe();
  const c9 = await clickAll(z9);
  await P.screenshot({ path: shot('zone2-bare-900.png') });
  check('ZONE2 bei 900 px: Knöpfe der App ohne Kit frei und per Maus klickbar', ['left', 'right', 'abs'].every((k) => z9.btns[k]?.free && c9[k] === 1), JSON.stringify({ z9, c9 }));
  await P.setViewportSize({ width: 1280, height: 800 });

  if (STUB) {
    // ---- Code-Ansicht: Kopf (Veröffentlichen, Verwerfen, Diff, …) frei im Rahmen-Wirt ----
    const notesSel = `#${await P.evaluate((o) => window.__lab.frame.shellHandle().win(o).handle.element.id, notes)}`;
    await P.evaluate((o) => { const h = window.__lab.frame.shellHandle(); const r = h.win(o); r.handle.element.style.left = '220px'; r.handle.element.style.top = '140px'; r.handle.element.style.width = '760px'; r.handle.element.style.height = '520px'; }, notes);
    const opened = await P.evaluate((o) => window.__lab.frame.shellHandle().win(o).host.instance.api.shell.openSource('notes'), notes);
    await P.waitForSelector(`${notesSel} [data-v4-source] [data-publish]`, { timeout: 8000 });
    await P.evaluate((sel) => { document.querySelector(sel).style.zIndex = '9999'; }, notesSel);
    await P.waitForTimeout(300);
    const srcProbe = () => P.$eval(notesSel, (w) => {
      const v = w.querySelector('[data-v4-source]');
      const pick = { publish: '[data-publish]', discard: '[data-discard]', diff: '[data-diff-toggle]', more: '[data-src-more]' };
      return { guard: v.getAttribute('data-v4-zone-guard'), btns: Object.fromEntries(Object.entries(pick).map(([k, s]) => { const b = v.querySelector(s); const r = b.getBoundingClientRect(); if (!(r.width > 0 && r.height > 0)) return [k, { hidden: true, free: true }]; const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2); return [k, { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2), free: !!hit && b.contains(hit), by: hit && !b.contains(hit) ? `${hit.tagName.toLowerCase()}.${String(hit.className).split(' ')[0]}` : null }]; })) };
    });
    const s1 = await srcProbe();
    await P.screenshot({ path: shot('zone2-source-1280.png') });
    check('ZONE2 Code-Ansicht 1280: Veröffentlichen, Verwerfen (sichtbar nur mit Entwurf), Diff und „…“ liegen frei (Rahmen-Wirt hält die Zone, keine zusätzliche Garantie nötig)',
      opened === true && Object.values(s1.btns).every((b) => b.free) && !s1.btns.publish.hidden && s1.guard === null, JSON.stringify(s1));
    await P.setViewportSize({ width: 900, height: 800 });
    await P.evaluate((sel) => { const w = document.querySelector(sel); w.style.left = '40px'; w.style.width = '760px'; }, notesSel);
    await P.waitForTimeout(300);
    const s9 = await srcProbe();
    await P.screenshot({ path: shot('zone2-source-900.png') });
    check('ZONE2 Code-Ansicht 900: Kopf frei', Object.values(s9.btns).every((b) => b.free), JSON.stringify(s9));
    await P.setViewportSize({ width: 1280, height: 800 });
    await P.evaluate((sel) => { const w = document.querySelector(sel); w.style.left = '220px'; }, notesSel);
    // ---- Wirt OHNE Zonen-Abstand (wie kernel-k1 openSource im K24-Lauf): die Ansicht hält die Zone selbst frei ----
    await P.evaluate((sel) => { document.querySelector(`${sel} > .v4-src-host`).style.paddingTop = '0px'; }, notesSel);
    await P.waitForTimeout(300);
    const sh = await srcProbe();
    await P.mouse.click(sh.btns.diff.x, sh.btns.diff.y);
    await P.waitForTimeout(150);
    const diffOn = await P.$eval(`${notesSel} [data-diff-toggle]`, (b) => b.getAttribute('aria-pressed'));
    await P.mouse.click(sh.btns.diff.x, sh.btns.diff.y);
    await P.screenshot({ path: shot('zone2-source-hostless-1280.png') });
    check('ZONE2 Code-Ansicht im Wirt ohne Zonen-Abstand: Garantie greift (top), Kopf frei, Diff per echter Maus umschaltbar',
      sh.guard === 'top' && Object.values(sh.btns).every((b) => b.free) && diffOn === 'true', JSON.stringify({ sh, diffOn }));
    await P.addStyleTag({ content: '.v4-src[data-v4-zone-guard] { padding-top: 0 !important; padding-right: 0 !important; }' });
    await P.waitForTimeout(100);
    const shm = await srcProbe();
    check('ZONE2 Gegenprobe Code-Ansicht: ohne die Garantie liegt „Veröffentlichen“ bzw. „…“ unter dem Knopfcluster', !shm.btns.publish.free || !shm.btns.more.free, JSON.stringify(shm));
  }
  check('ZONE2 ohne Seitenfehler', perr.length === 0, perr.join(' | '));
  await ctx.close();
}

// ================= MOBILE (WJB-M): Workjet-Mobil-Host – html[data-workjet-mobile-host="true"] =================
// M-09 (Bedienkonzept §Icons „Pflicht beim Hochladen“): Ordner-Upload ohne Icon wird nicht veröffentlicht; der Grund
// steht als Fehler in der Zone des Neue-App-Fensters. Nur Stub (kernel.source). M09_ONLY=1 lässt nur diesen Block laufen.
async function m09() {
  if (!STUB) return;
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const N = await ctx.newPage();
  const nerr = [];
  N.on('pageerror', (e) => nerr.push(String(e)));
  await N.goto(`${base}/v4/frame/lab/stub-lab.html?inst=m09-${Date.now().toString(36)}`);
  await N.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  await N.click('[data-shell-start]');
  await N.waitForSelector('[data-v4-launcher-action="new-app"]');
  await N.click('[data-v4-launcher-action="new-app"]');
  await N.waitForSelector('.shell-window[data-window-view="new-app"] [data-v4-source-upload]');
  await N.setInputFiles('[data-v4-source-upload] input[type=file]', path.join(here, 'upload/ohne-icon'));
  await N.waitForSelector('[data-v4-source-upload] [data-files] li');
  await N.waitForTimeout(700); // Öffnungsanimation des Fensters abwarten (wie FR3)
  const btn = await N.$eval('[data-v4-source-upload] [data-publish]', (b) => { const r = b.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2, disabled: b.disabled }; });
  btn.hit = await N.evaluate(({ x, y }) => { const e = document.elementFromPoint(x, y); return e ? (e.closest('[data-publish]') ? 'publish' : `${e.tagName}.${e.className}`) : null; }, btn);
  btn.src = await N.evaluate(() => performance.getEntriesByType('resource').map((e) => e.name).find((n) => n.includes('upload.js')) || null);
  await N.mouse.click(btn.x, btn.y);
  await N.waitForTimeout(2600); // Zonen-Badge erscheint verzögert (wie Meldungs-Badge)
  const bpos = await N.$eval('.shell-window[data-window-view="new-app"] [data-v4-zone-slot] .v4-status-zone__badge', (b) => { const r = b.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; }).catch(() => null);
  if (bpos) { await N.mouse.click(bpos[0], bpos[1]); await N.waitForTimeout(300); } // Liste wie ein Nutzer aufklappen (M-14)
  const r = await N.evaluate(() => {
    const k = window.__lab.kernel;
    const win = document.querySelector('.shell-window[data-window-view="new-app"]');
    const badge = win?.querySelector('[data-v4-zone-slot] .v4-status-zone__badge');
    return { published: k.apps.get().some((a) => a.id === 'ohne-icon'), last: document.querySelector('[data-v4-source-upload]')?.dataset.lastPublished || '',
      zoneText: win?.querySelector('.v4-status-zone__list')?.textContent?.trim() || '', badge: badge ? { text: badge.textContent.trim(), hidden: badge.hidden || getComputedStyle(badge).display === 'none' } : null };
  });
  await N.screenshot({ path: shot('m09-upload-ohne-icon-1280.png') });
  check('M-09 Upload ohne Icon: nicht veröffentlicht, Grund „Icon fehlt“ in der Zone des Fensters (echter Mausklick auf Veröffentlichen)',
    btn.hit === 'publish' && !btn.disabled && !r.published && r.last === '' && /Icon fehlt/.test(r.zoneText) && r.badge && !r.badge.hidden, JSON.stringify({ btn, r }));
  check('M-09 ohne Seitenfehler', nerr.length === 0, nerr.join(' | '));
  await ctx.close();
}

// ICON25 (ZONE2-Nebenbefund): Fenster-Icon = Glyphengröße des Desktop-Launchers (V2: prepareShellV2IconGeometry), egal
// ob die App über Startmenü, Desktop oder Reiter geöffnet wird. Echte Maus. ICON25_ONLY=1 lässt nur diesen Block laufen.
async function icon25() {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const P = await ctx.newPage();
  const perr = [];
  P.on('pageerror', (e) => perr.push(String(e)));
  await P.goto(`${base}/v4/frame/lab/${STUB ? `stub-lab.html?inst=icon25-${Date.now().toString(36)}` : 'index.html'}`);
  await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  const glyph = () => P.evaluate(() => { const g = document.querySelector('[data-v4-desktop] .desktop-icon-glyph'); return g ? Math.round(g.getBoundingClientRect().width) : null; });
  const winIcon = (app) => P.evaluate((app) => {
    const w = [...document.querySelectorAll('.shell-window')].find((e) => (e.dataset.windowApp ?? e.dataset.app) === app && e.style.display !== 'none');
    if (!w) return null;
    const i = w.querySelector('.shell-window-v2-icon');
    return { var: getComputedStyle(w).getPropertyValue('--shell-v2-icon-size').trim(), icon: i ? Math.round(i.getBoundingClientRect().width) : null };
  }, app);
  const g0 = await glyph();
  // Startmenü: echter Klick auf den ersten App-Eintrag
  await P.mouse.click(...(await P.$eval('[data-shell-start]', (e) => { const r = e.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })));
  await P.waitForSelector('[data-v4-launcher].is-active [data-v4-icon]', { timeout: 3000 });
  const tile = await P.$eval('[data-v4-launcher].is-active [data-v4-icon]', (e) => { const t = e.closest('[data-v4-app], [data-app], [data-app-id]'); const r = e.getBoundingClientRect(); return { app: t?.dataset.v4App ?? t?.dataset.app ?? t?.dataset.appId ?? null, w: Math.round(r.width), x: r.left + r.width / 2, y: r.top + r.height / 2 }; });
  await P.mouse.click(tile.x, tile.y);
  await P.waitForTimeout(900);
  const fromStart = tile.app ? await winIcon(tile.app) : null;
  await P.screenshot({ path: shot('icon25-start-1280.png') });
  check(`ICON25 Fenster aus dem Startmenü (Eintrag ${tile.w} px) trägt die Launcher-Glyphengröße (${g0} px) wie V2, nicht die Größe des Eintrags`,
    !!tile.app && g0 >= 56 && g0 <= 80 && fromStart?.var === `${g0}px` && Math.abs((fromStart?.icon ?? 0) - g0) <= 1 && tile.w < g0, JSON.stringify({ g0, tile, fromStart }));
  check('ICON25 ohne Seitenfehler', perr.length === 0, perr.join(' | '));
  await ctx.close();
}

// Eigene Seite je Breite. MOBILE_ONLY=1 lässt nur diesen Block laufen. Nur mock: der Mobil-Katalog der Hostbrücke ist
// eine Momentaufnahme der ersten App-Liste (index.js mobileCatalogOf); die Labor-Seite liefert deshalb Apps MIT layout
// (layout_known, wie kernel.apps §16.19 Nr. 6) und layout.presentation.mobile schon beim Start. Die Seite entsteht hier
// per route (wie die Z45-Probe-App) aus kernel-mock.js und ../index.js; lab/index.html bleibt unverändert.
// Attribut vor jedem Skript (addInitScript), Hostbefehle als CustomEvent 'workjet-business-os-host-command'
// (Protokoll workjet.business-os-shell.v1, host-bridge.js receive), Antworten über workjetBusinessOsPostMessage.
const MOBILE_PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Mobil-Labor</title><style>html,body{margin:0;height:100%}</style></head><body><div id="ctox-shell"></div><script type="module">
import { createKernelMock } from './kernel-mock.js';
import * as frame from '../index.js';
const mob = (title, mobilePresentation, extra = {}) => ({ title, mobilePresentation, phoneReady: true, tabletReady: true, ...extra });
const L = {
  notes: { icon_asset: 'icon.svg', category: 'Arbeit', presentation: { mobile: mob('Notizen', 'document') } },
  tasks: { icon_asset: 'icon.svg', category: 'Arbeit', presentation: { mobile: mob('Aufgaben', 'list-detail') } },
  ledger: { icon_asset: 'icon.svg', category: 'Finanzen', presentation: { mobile: mob('Buchhaltung', 'list-detail', { desktopOnly: true }) } },
  bare: { category: 'Arbeit', default_width: 760, default_height: 520, presentation: { mobile: mob('Fremde App', 'form') } },
  crm: { icon_asset: 'icon.svg', category: 'Vertrieb', presentation: { mobile: mob('Kontakte', 'list-detail') } },
  studio: { category: 'Arbeit', presentation: { mobile: mob('Black Hole Studio', 'canvas') } },
  alpha: { category: 'Labor' }, beta: { category: 'Labor' },
  mira: { category: 'Crew', presentation: { taskbar_group: 'crew', taskbar_group_title: 'Crew' } },
};
const ver = { crm: '2.0.0' };
const title = { notes: 'Notizen', tasks: 'Aufgaben', ledger: 'Buchhaltung', bare: 'Fremde App', crm: 'Kontakte', studio: 'Black Hole Studio', alpha: 'Alpha', beta: 'Beta', mira: 'Mira' };
const rows = Object.entries(L).map(([id, layout]) => { const v = ver[id] || '1.0.0'; return { id, title: title[id], active_ver: v, activated_ver: v, state: 'activated', target_ver: null, error: null, layout_known: true, layout }; });
const kernel = createKernelMock({ apps: rows, manifests: Object.fromEntries(rows.map((r) => [r.id + '@' + r.active_ver, { app_id: r.id, app_ver: r.active_ver, entry: 'index.js', layout: r.layout }])) });
window.__lab = { kernel, frame };
await frame.startShell(kernel, {});
window.__labReady = true;
</script></body></html>`;
async function mobile() {
  if (STUB) return;
  const host = (P, detail) => P.evaluate((d) => dispatchEvent(new CustomEvent('workjet-business-os-host-command', { detail: { protocol: 'workjet.business-os-shell.v1', ...d } })), detail);
  const openPage = async (w, h, { attr = true, reduced = false } = {}) => {
    const ctx = await browser.newContext({ viewport: { width: w, height: h }, reducedMotion: reduced ? 'reduce' : 'no-preference' });
    const P = await ctx.newPage();
    const perr = [];
    P.on('pageerror', (e) => perr.push(String(e)));
    P.on('console', (m) => { if (m.type() === 'error' && !/404|Failed to load resource/.test(m.text())) perr.push(m.text()); });
    await P.addInitScript((attr) => {
      window.__mPosts = [];
      window.workjetBusinessOsPostMessage = (m) => window.__mPosts.push(m);
      if (!attr) return;
      // Vor jedem Skript wie der Workjet-Host (WorkjetBusinessOsModule): documentElement existiert evtl. noch nicht.
      const set = () => { const el = document.documentElement; if (!el) return false; el.dataset.workjetMobileHost = 'true'; return true; };
      if (!set()) new MutationObserver((_, o) => { if (set()) o.disconnect(); }).observe(document, { childList: true });
    }, attr);
    await P.route(/\/v4\/frame\/lab\/mobile-lab\.html$/, (route) => route.fulfill({ status: 200, contentType: 'text/html', body: MOBILE_PAGE }));
    await P.goto(`${base}/v4/frame/lab/mobile-lab.html`);
    await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
    await P.waitForTimeout(300);
    return { ctx, P, perr };
  };
  // Sichtbarkeit je Rahmenteil: Rechteck (Geometrie) + display/visibility der Kette (getComputedStyle) + checkVisibility.
  const chrome = (P) => P.evaluate(() => {
    const parts = { top: '[data-v4-topbar]', topbar: '[data-v4-topbar] .topbar', tabs: '[data-v4-topbar] .module-tabs', start: '[data-shell-start]', launcher: '[data-v4-launcher]', crew: '[data-v4-crew-bar]' };
    return Object.fromEntries(Object.entries(parts).map(([k, s]) => {
      const el = document.querySelector(s);
      if (!el) return [k, { missing: true, shown: false }];
      const r = el.getBoundingClientRect();
      let hidden = null;
      for (let a = el; a && a !== document.documentElement; a = a.parentElement) { const cs = getComputedStyle(a); if (cs.display === 'none' || cs.visibility === 'hidden') { hidden = `${a.className || a.tagName}:${cs.display === 'none' ? 'display' : 'visibility'}`; break; } }
      const area = Math.round(r.width * r.height);
      return [k, { area, hidden, shown: !hidden && area > 0 && el.checkVisibility({ visibilityProperty: true, opacityProperty: true }) }];
    }));
  });
  // Desktop-Raster aus den Symbol-Rechtecken: Spalten = Symbole in der ersten Zeile; zweite Zeile beginnt links bündig.
  // Zeilen nach der Layout-Lage (offsetTop/offsetLeft): Hover hebt ein Symbol per transform um 2 px an (V2 :hover).
  const grid = (P) => P.evaluate(() => {
    const icons = [...document.querySelectorAll('[data-v4-desktop] .desktop-icon[data-target]')].map((e) => { const r = e.getBoundingClientRect(); return { id: e.dataset.target, x: e.offsetLeft, y: e.offsetTop, r: Math.round(r.right), b: Math.round(r.bottom), w: Math.round(r.width), h: Math.round(r.height), cx: r.left + r.width / 2, cy: r.top + r.height / 2 }; });
    const firstY = icons[0]?.y;
    const row1 = icons.filter((i) => i.y === firstY);
    const row2 = icons.filter((i) => i.y > firstY);
    const desk = document.querySelector('[data-v4-desk]').getBoundingClientRect();
    const c = icons[0] ? document.elementFromPoint(icons[0].cx, icons[0].cy) : null;
    const top = icons[0] ? Math.round(icons[0].cy - icons[0].h / 2) : null;
    return { n: icons.length, cols: row1.length, wrapX: row2[0] ? row2[0].x === icons[0].x : null, inside: icons.every((i) => i.cx - i.w / 2 >= 0 && i.r <= innerWidth), top,
      desk: [Math.round(desk.left), Math.round(desk.top), Math.round(desk.width), Math.round(desk.height)], hit: !!c?.closest('.desktop-icon'), minH: Math.min(...icons.map((i) => i.h)) };
  });
  const wins = (P) => P.evaluate(() => [...document.querySelectorAll('.v4f .shell-window')].map((w) => {
    const r = w.getBoundingClientRect();
    const part = (s) => { const e = w.querySelector(s); if (!e) return null; const q = e.getBoundingClientRect(); return e.checkVisibility() && q.width * q.height > 0; };
    return { app: w.dataset.windowApp, active: w.dataset.workjetMobileActive === 'true', shown: w.checkVisibility(), rect: [r.left, r.top, r.width, r.height].map(Math.round),
      head: part(':scope > .shell-window-header'), icon: part(':scope > .shell-window-v2-icon'), resize: [...w.querySelectorAll(':scope > .shell-window-resize')].some((e) => e.checkVisibility()), zone: part(':scope > .v4f-zone-slot') };
  }));
  const states = (P) => P.evaluate(() => window.__mPosts.filter((m) => m.type === 'app.state').map((m) => `${m.appId}:${m.state}`));
  const off = (c) => Object.entries(c).filter(([, v]) => v.shown).map(([k]) => k);

  for (const [w, h, cols] of [[390, 844, 4], [600, 960, 5], [900, 1200, 6]]) {
    const { ctx, P, perr } = await openPage(w, h);
    const ready = await P.evaluate(() => ({ bridge: !!window.workjetHostBridge, posts: window.__mPosts.map((m) => m.type), apps: (window.__mPosts.find((m) => m.type === 'catalog.replace')?.catalog.apps || []).map((a) => a.id) }));
    check(`MOBILE ${w}: Hostbrücke im Labor eingehängt (workjetHostBridge, shell.ready + catalog.replace mit den Mobil-Apps aus layout.presentation.mobile)`,
      ready.bridge && ready.posts.includes('shell.ready') && ['notes', 'bare', 'crm'].every((id) => ready.apps.includes(id)) && !ready.apps.includes('alpha'), JSON.stringify(ready));
    // ---- Schritt 1: Start = Desktop ohne doppelte Chrome, Touch-Raster ----
    await host(P, { type: 'app.open', appId: 'desktop' });
    await P.waitForTimeout(150);
    const c1 = await chrome(P);
    const g1 = await grid(P);
    await P.screenshot({ path: shot(`mobile-desktop-${w}.png`) });
    check(`MOBILE ${w} Schritt 1: Kopfleiste (mit Reitern), Startknopf, Startmenü und Crew-Leiste unsichtbar (Fläche 0 bzw. display none, checkVisibility false)`,
      off(c1).length === 0 && c1.top.area === 0 && c1.topbar.area === 0 && c1.crew.area === 0, JSON.stringify(c1));
    check(`MOBILE ${w} Schritt 1: Desk füllt den Viewport, Desktop-Raster mit ${cols} Spalten (Symbol-Rechtecke, Umbruch links bündig, alle im Viewport, per Treffertest erreichbar)`,
      g1.desk.join() === `0,0,${w},${h}` && g1.top < 48 && g1.cols === cols && g1.wrapX === true && g1.inside && g1.hit && g1.minH >= 44, JSON.stringify(g1));
    // ---- Schritt 2: App über die Hostbrücke öffnen (zuerst eine zweite, die danach verborgen sein muss) ----
    await host(P, { type: 'app.open', appId: 'notes' });
    await P.waitForSelector('.shell-window[data-window-app="notes"][data-workjet-mobile-active="true"]', { timeout: 5000 }).catch(() => null);
    await host(P, { type: 'app.open', appId: 'bare' });
    await P.waitForSelector('.shell-window[data-window-app="bare"][data-workjet-mobile-active="true"] [data-bare="left"]', { timeout: 5000 }).catch(() => null);
    await P.waitForTimeout(450);
    const w2 = await wins(P);
    const active = w2.filter((x) => x.active);
    const a = active[0];
    check(`MOBILE ${w} Schritt 2: genau ein aktives Fenster (bare), Rechteck = Viewport ±1 px`,
      active.length === 1 && a.app === 'bare' && a.shown && Math.abs(a.rect[0]) <= 1 && Math.abs(a.rect[1]) <= 1 && Math.abs(a.rect[2] - w) <= 1 && Math.abs(a.rect[3] - h) <= 1, JSON.stringify(w2));
    check(`MOBILE ${w} Schritt 2: Fensterkopf (Icon-Header, Kopfzeile mit Knopfcluster), Größe-Griffe und Meldungs-Badge unsichtbar`,
      a && a.head === false && a.icon === false && a.resize === false && a.zone === false, JSON.stringify(a));
    const others = w2.filter((x) => !x.active);
    check(`MOBILE ${w} Schritt 2: andere Fenster (notes) verborgen`, others.length >= 1 && others.some((x) => x.app === 'notes') && others.every((x) => !x.shown), JSON.stringify(others));
    const c2 = await chrome(P);
    check(`MOBILE ${w} Schritt 2: Chrome bleibt aus, solange eine App aktiv ist`, off(c2).length === 0, JSON.stringify(off(c2)));
    // Knöpfe der App: Mittelpunkt frei (elementFromPoint), echte Maus, Touch-Mindesthöhe 44 px (V2-Regel im Fensterinhalt).
    const btns = await P.$$eval('.shell-window[data-workjet-mobile-active="true"] [data-bare]', (bs) => bs.map((b) => { const r = b.getBoundingClientRect(); const x = r.left + r.width / 2; const y = r.top + r.height / 2; const hit = document.elementFromPoint(x, y); return { k: b.dataset.bare, x: Math.round(x), y: Math.round(y), h: Math.round(r.height), free: !!hit && b.contains(hit), by: hit && !b.contains(hit) ? `${hit.tagName.toLowerCase()}.${String(hit.className).split(' ')[0]}` : null }; }));
    await P.evaluate(() => { globalThis.__bareClicks = {}; });
    for (const b of btns) await P.mouse.click(b.x, b.y);
    await P.waitForTimeout(80);
    const clicks = await P.evaluate(() => ({ ...globalThis.__bareClicks }));
    const pres = await P.evaluate(() => ({ app: document.body.dataset.workjetMobileApp, pres: document.body.dataset.workjetMobilePresentation }));
    check(`MOBILE ${w} Schritt 2: Knöpfe der App frei (elementFromPoint am Mittelpunkt), per echter Maus getroffen, je ≥ 44 px hoch; body trägt App und Darstellung`,
      btns.length === 4 && btns.every((b) => b.free && b.h >= 44 && clicks[b.k] === 1) && pres.app === 'bare' && pres.pres === 'form', JSON.stringify({ btns, clicks, pres }));
    await P.screenshot({ path: shot(`mobile-app-${w}.png`) });
    // ---- Schritt 3: zurück zum Desktop per Hostbefehl (v4 hat keinen Hash-Router; Bridge → shell().showDesktop()) ----
    await host(P, { type: 'app.open', appId: 'desktop' });
    await P.waitForTimeout(450);
    const w3 = await wins(P);
    const g3 = await grid(P);
    const c3 = await chrome(P);
    await P.screenshot({ path: shot(`mobile-home-${w}.png`) });
    check(`MOBILE ${w} Schritt 3: Hostbefehl Desktop → alle Fenster verborgen, Raster wieder sichtbar (${cols} Spalten, Symbol per Treffertest erreichbar), Chrome bleibt aus`,
      w3.length >= 2 && w3.every((x) => !x.shown) && g3.cols === cols && g3.hit && off(c3).length === 0, JSON.stringify({ w3, g3 }));
    // WJB-M-1 (Hostbrücke, nicht Rahmen-CSS): openApp('desktop') räumt die Marke des zuletzt aktiven Fensters nicht
    // ab (markActiveWindow läuft nur über childList-Mutationen; Minimieren ändert nur style). Sichtbar folgenlos (Fenster
    // minimiert), aber der Zustand lügt. Behoben in host-bridge.js (queueMicrotask(markActiveWindow) nach showDesktop).
    const stale = w3.filter((x) => x.active).map((x) => x.app);
    check(`MOBILE ${w} (WJB-M-1): nach Hostbefehl Desktop trägt kein Fenster mehr die Aktiv-Marke`, stale.length === 0, JSON.stringify(stale));
    // ---- WJB-M-2: Symbol antippen (echte Maus) → App sollte vollflächig erscheinen (host-bridge onClick) ----
    // Heute: der Fenstermanager lässt is-focused am minimierten Fenster stehen, markActiveWindow nimmt das ERSTE
    // .is-focused im DOM und befördert die zuvor aktive App zurück (Workjet erhält deren app.state, die getippte App bleibt
    // verborgen). Behoben in host-bridge.js: nur das oberste sichtbare fokussierte Fenster zählt.
    const icon = await P.$eval('[data-v4-desktop] .desktop-icon[data-target="crm"]', (e) => { const r = e.getBoundingClientRect(); return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) }; });
    await P.mouse.click(icon.x, icon.y);
    await P.waitForSelector('.shell-window[data-window-app="crm"][data-workjet-mobile-active="true"]', { timeout: 5000 }).catch(() => null);
    await P.waitForTimeout(450);
    const w4 = await wins(P);
    const a4 = w4.filter((x) => x.active);
    const st = await states(P);
    const tapOk = a4.length === 1 && a4[0].app === 'crm' && a4[0].shown && Math.abs(a4[0].rect[2] - w) <= 1 && Math.abs(a4[0].rect[3] - h) <= 1
      && w4.filter((x) => !x.active).every((x) => !x.shown) && st.at(-1) === 'crm:active';
    check(`MOBILE ${w} (WJB-M-2): Symbol antippen (echte Maus) → getippte App vollflächig aktiv, Workjet erhält ihren app.state`, tapOk, JSON.stringify({ a4, st: st.slice(-3) }));
    check(`MOBILE ${w}: Workjet erhält app.state für Öffnen (opening → active) und Desktop`, ['notes:opening', 'notes:active', 'bare:opening', 'bare:active', 'desktop:active'].every((s) => st.includes(s)), JSON.stringify(st));
    if (w === 390) await P.screenshot({ path: shot(`mobile-icon-tap-${w}.png`) }); // Beleg für WJB-M-2
    check(`MOBILE ${w} ohne Seitenfehler`, perr.length === 0, perr.join(' | '));
    await ctx.close();
  }

  // ---- prefers-reduced-motion: Übergänge und Animationen im Mobil-Host auf 0,01 ms (V2) ----
  {
    const { ctx, P, perr } = await openPage(390, 844, { reduced: true });
    await host(P, { type: 'app.open', appId: 'bare' });
    await P.waitForSelector('.shell-window[data-window-app="bare"][data-workjet-mobile-active="true"]', { timeout: 5000 }).catch(() => null);
    const rm = await P.evaluate(() => {
      const d = (e) => (e ? `${getComputedStyle(e).transitionDuration}|${getComputedStyle(e).animationDuration}` : null);
      return { win: d(document.querySelector('.shell-window[data-workjet-mobile-active]')), icon: d(document.querySelector('[data-v4-desktop] .desktop-icon')), btn: d(document.querySelector('[data-bare="left"]')) };
    });
    check('MOBILE prefers-reduced-motion: Fenster, Desktop-Symbol und App-Knopf mit Übergangs-/Animationsdauer 0,01 ms', Object.values(rm).every((v) => v && v.split('|').every((x) => x.split(',').every((y) => y.trim() === '1e-05s'))), JSON.stringify({ rm, perr }));
    await ctx.close();
  }

  // ---- Gegenprobe: dieselbe Seite OHNE Attribut → Kopfleiste (Reiter, Startknopf) und Crew-Leiste wie immer ----
  for (const [w, h] of [[390, 844], [900, 1200]]) {
    const { ctx, P, perr } = await openPage(w, h, { attr: false });
    await P.evaluate(() => { const h = window.__lab.frame.shellHandle(); h.openWindow('bare', { geometry: { x: 40, y: 40, w: 600, h: 400 } }); });
    await P.waitForTimeout(400);
    const c = await chrome(P);
    const w0 = await wins(P);
    const ready = await P.evaluate(() => ({ bridge: !!window.workjetHostBridge, posts: window.__mPosts.length, body: document.body.dataset.workjetMobileApp ?? null }));
    await P.screenshot({ path: shot(`mobile-control-${w}.png`) });
    check(`MOBILE Gegenprobe ${w} ohne Attribut: Kopfleiste, Reiter, Startknopf und Crew-Leiste sichtbar; Fenster mit Kopf; keine Hostbrücke`,
      c.top.shown && c.topbar.shown && c.tabs.shown && c.start.shown && c.crew.shown && !c.launcher.shown && w0[0]?.shown && w0[0]?.head === true && !w0[0]?.active && !ready.bridge && ready.posts === 0 && perr.length === 0,
      JSON.stringify({ c, w0, ready, perr }));
    await ctx.close();
  }
}

// ================= TOPBAR: Kopfleiste der Shell-V2 im v4-Rahmen (eigene Seite; TOP_ONLY=1 → nur dieser Block) =================
const pxCompare = (P, a, b, regions) => P.evaluate(async ([a, b, regions]) => {
  const load = async (b64) => { const i = new Image(); i.src = `data:image/png;base64,${b64}`; await i.decode(); const c = document.createElement('canvas'); c.width = i.naturalWidth; c.height = i.naturalHeight; const g = c.getContext('2d'); g.drawImage(i, 0, 0); return g; };
  const [ga, gb] = [await load(a), await load(b)];
  const out = {};
  for (const [k, [x, y, w, h]] of Object.entries(regions)) {
    const da = ga.getImageData(x, y, w, h).data; const db = gb.getImageData(x, y, w, h).data;
    let same = 0; for (let i = 0; i < da.length; i += 4) if (Math.abs(da[i] - db[i]) + Math.abs(da[i + 1] - db[i + 1]) + Math.abs(da[i + 2] - db[i + 2]) <= 24) same++;
    out[k] = +(same / (da.length / 4)).toFixed(4);
  }
  return out;
}, [a.toString('base64'), b.toString('base64'), regions]);

async function topbarLab() {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const P = await ctx.newPage();
  const perr = [];
  P.on('pageerror', (e) => perr.push(String(e)));
  P.on('console', (m) => { if (m.type() === 'error' && !/404|Failed to load resource/.test(m.text())) perr.push(m.text()); });
  await P.goto(`${base}/v4/frame/lab/${STUB ? `stub-lab.html?inst=top-${Date.now().toString(36)}` : 'index.html'}`);
  await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  const appIds = await P.$$eval('[data-v4-launcher] [data-v4-app]', (ts) => ts.map((t) => t.dataset.v4App));
  const open = (id) => P.evaluate(async (id) => { const h = window.__lab.frame.shellHandle(); const o = h.openWindow(id); await h.win(o).host.ready; return o; }, id);
  const o1 = await open(appIds[0]);
  const o2 = await open(appIds[1]);
  await P.waitForFunction(() => document.querySelectorAll('.topbar .module-tabs .module-tab').length >= 2, null, { timeout: 5000 }).catch(() => {});
  await P.waitForTimeout(200);

  // ---- TOP-0 Aufbau: Kopfleiste oben (48 px), Desk darunter, Reiter je offener App, Workspace und Avatar ----
  const lay = await P.evaluate(() => {
    const top = document.querySelector('[data-v4-topbar] .topbar').getBoundingClientRect();
    const desk = document.querySelector('[data-v4-desk]').getBoundingClientRect();
    const tabs = [...document.querySelectorAll('.topbar .module-tabs .module-tab')].map((t) => ({ app: t.dataset.v4TabApp, focused: t.dataset.running === 'focused' }));
    return { top: [top.top, top.height], deskTop: desk.top, tabs, ws: document.querySelector('[data-shell-instance-name]').textContent, avatar: document.querySelector('.account-button').dataset.avatar || '',
      order: [...document.querySelector('.topbar-actions').children].map((c) => c.dataset.v4Sync !== undefined ? 'sync' : c.dataset.v4SettingsButton !== undefined ? 'gear' : c.matches('.account-button') ? 'avatar' : c.className) };
  });
  check('TOP-0 Kopfleiste wie Shell-V2: 48 px oben, Desk beginnt darunter, je offener App ein Reiter (vorderste markiert), Workspace-Name, rechts Punkt → Zahnrad → Avatar',
    lay.top[0] === 0 && lay.top[1] === 48 && lay.deskTop === 48 && lay.tabs.length === 2 && lay.tabs.filter((t) => t.focused).length === 1 && !!lay.ws && lay.order.join(',') === 'sync,gear,avatar', JSON.stringify(lay));
  // Reiter-Klick holt das Fenster nach vorn
  const firstTab = `.topbar .module-tab[data-v4-tab-app="${appIds[0]}"]`;
  await P.click(firstTab);
  await P.waitForTimeout(150);
  const front = await P.evaluate((o) => window.__lab.frame.shellHandle().win(o).handle.element.classList.contains('is-focused'), o1);
  check('TOP-0 Reiter-Klick fokussiert das Fenster der App', front && (await P.$eval(firstTab, (t) => t.dataset.running === 'focused')));
  // FR6d: Launcher-Raster = Startmenü der Shell-V2 (Popup), zweiter Klick schließt es; Fenster bleiben unberührt.
  const menuOpen = () => P.$eval('[data-v4-launcher]', (e) => e.classList.contains('is-active'));
  const visWins = () => P.evaluate(() => window.__lab.frame.shellHandle().windows().filter((r) => r.handle.element.style.display !== 'none' && !r.handle.element.classList.contains('is-minimized')).length);
  const w0 = await visWins();
  await P.click('[data-v4-topbar] [data-shell-start]');
  await P.waitForTimeout(250);
  const m1 = await menuOpen();
  await P.click('[data-v4-topbar] [data-shell-start]');
  await P.waitForTimeout(250);
  const m2 = await menuOpen();
  check('TOP-0 Launcher-Raster öffnet das Startmenü und schließt es beim zweiten Klick (Fenster unberührt)', m1 && !m2 && (await visWins()) === w0, JSON.stringify({ m1, m2, w0 }));

  // ---- TOP-1 G0 Kopfleiste: v4-Rahmen gegen den app.js-Pfad (index.html-Stylesheets) mit demselben DOM, hell + dunkel ----
  const g0 = async (theme, width, mutate = null) => {
    await P.setViewportSize({ width, height: 800 });
    await P.evaluate((th) => { document.documentElement.dataset.theme = th; }, theme);
    await P.mouse.move(width / 2, 600);
    await P.evaluate(() => document.activeElement?.blur?.()); // kein Fokus-/Hover-Zustand im Vergleich
    await P.waitForTimeout(400);
    // Nur die Kopfleiste vergleichen: Desk/Taskleiste ausblenden (Hintergrund bleibt der Rahmen-Hintergrund).
    await P.evaluate(() => { for (const el of document.querySelectorAll('[data-v4-desk], .v4f-bar')) el.style.visibility = 'hidden'; });
    await P.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
    // DOM-Abzug unmittelbar am Bild (Reiter-Neuanordnung dazwischen ausgeschlossen); Mutation erst danach.
    // FR6c: das Zahnrad ist unter 560 px ausgeblendet (welsch hat gar keins); die Referenz bekommt das sichtbare DOM.
    const cap = await P.evaluate(() => {
      const shell = document.querySelector('[data-v4-topbar] > .app-shell');
      const clone = shell.cloneNode(true);
      if (getComputedStyle(shell.querySelector('[data-v4-settings-button]')).display === 'none') clone.querySelector('[data-v4-settings-button]').remove();
      return { html: clone.innerHTML, bg: getComputedStyle(document.querySelector('[data-v4-frame]')).backgroundColor };
    });
    if (mutate) await P.evaluate(mutate);
    const v4png = await P.screenshot({ clip: { x: 0, y: 0, width, height: 48 } });
    await P.evaluate(() => { for (const el of document.querySelectorAll('[data-v4-desk], .v4f-bar')) el.style.visibility = ''; });
    const R = await ctx.newPage();
    await R.setViewportSize({ width, height: 800 });
    await R.goto(`${base}/v4/frame/lab/index.html?top-g0=1`).catch(() => {});
    await R.setContent(`<!doctype html><html lang="de" data-theme="${theme}" data-shell-style="ctox"><head><meta charset="utf-8">
      <link rel="stylesheet" href="${base}/ui-contract/v1/workjet-ui-contract.css"><link rel="stylesheet" href="${base}/app.css"><link rel="stylesheet" href="${base}/shared/base.css">
      <link rel="stylesheet" href="${base}/mobile-host.css"><link rel="stylesheet" href="${base}/shared/shell-topbar.css">
      <style>html,body{background:${cap.bg} !important} .app-shell{background:transparent !important}</style></head>
      <body data-module-shell="full"><div class="app-shell">${cap.html}</div></body></html>`, { waitUntil: 'load' });
    await R.mouse.move(width / 2, 600);
    await R.waitForTimeout(400);
    const refPng = await R.screenshot({ clip: { x: 0, y: 0, width, height: 48 } });
    await R.close();
    const third = Math.floor(width / 3);
    const cmp = await pxCompare(P, v4png, refPng, { left: [0, 0, third, 48], mid: [third, 0, third, 48], right: [2 * third, 0, width - 2 * third, 48] });
    return { cmp, v4png, refPng };
  };
  const res = {};
  for (const [theme, width] of [['light', 1280], ['dark', 1280], ['dark', 420]]) {
    const r = await g0(theme, width);
    res[`${theme}-${width}`] = r.cmp;
    fs.writeFileSync(shot(`top-g0-v4-${theme}-${width}.png`), r.v4png);
    fs.writeFileSync(shot(`top-g0-ref-appcss-${theme}-${width}.png`), r.refPng);
  }
  check('TOP-1 G0 Kopfleiste = app.js-Pfad (ui-contract + app.css + base.css + mobile-host.css + shell-topbar.css) mit demselben DOM, hell/dunkel 1280 und 420 (≥ 98 % Pixel je Drittel)',
    Object.values(res).every((c) => Object.values(c).every((v) => v >= 0.98)), JSON.stringify(res));
  // Mutation: verschobene Kopfleiste (Innenabstand links 40 px) muss auffallen.
  const mut = await g0('light', 1280, () => { document.querySelector('[data-v4-topbar] .topbar').style.setProperty('padding-left', '40px', 'important'); });
  await P.evaluate(() => { document.querySelector('[data-v4-topbar] .topbar').style.removeProperty('padding-left'); });
  check('TOP-1 Mutationsprobe: verschobene Kopfleiste weicht ab (< 98 %)', mut.cmp.left < 0.98, JSON.stringify(mut.cmp));
  await P.setViewportSize({ width: 1280, height: 800 });
  await P.evaluate(() => { document.documentElement.dataset.theme = 'light'; });

  // ---- TOP-2 Zahnrad öffnet die Einstellungen (Sprache, Schema); Schema wirkt sofort und wird gespeichert; Esc schließt ----
  const settingsState = () => P.evaluate(() => {
    const s = document.querySelector('[data-v4-settings-panel]');
    const r = s.getBoundingClientRect();
    return { open: !s.hidden && r.width > 200 && r.right <= innerWidth + 1, selects: [...s.querySelectorAll('select')].map((x) => x.dataset.languageSelect !== undefined ? 'lang' : x.dataset.themeSelect !== undefined ? 'theme' : '?'), title: s.querySelector('h2')?.textContent };
  });
  await P.click('[data-v4-settings-button]');
  await P.waitForTimeout(150);
  const s1 = await settingsState();
  await P.selectOption('[data-v4-settings-panel] [data-theme-select]', 'dark');
  await P.waitForTimeout(200);
  const themeNow = await P.evaluate(() => document.documentElement.dataset.theme);
  const saved = await P.evaluate(async () => { const h = window.__lab.frame.shellHandle(); await h.preferencesWritten?.(); await new Promise((r) => setTimeout(r, 100)); const p = h.kernel.preferences?.('dev'); try { await p?.load?.(); } catch {} return p?.get?.()?.settings?.theme ?? null; });
  await P.keyboard.press('Escape');
  await P.waitForTimeout(100);
  const s2 = await settingsState();
  const focusBack = await P.evaluate(() => document.activeElement?.dataset?.v4SettingsButton !== undefined);
  check('TOP-2 Zahnrad links vom Avatar öffnet „Einstellungen“ (Blatt mit Sprache und Schema); Schema „Dunkel“ wirkt sofort und steht in shell_preferences.settings; Esc schließt, Fokus zurück',
    s1.open && s1.selects.join(',') === 'lang,theme' && s1.title === 'Einstellungen' && themeNow === 'dark' && saved === 'dark' && !s2.open && focusBack, JSON.stringify({ s1, themeNow, saved, s2, focusBack }));
  await P.click('[data-v4-settings-button]');
  await P.selectOption('[data-v4-settings-panel] [data-theme-select]', 'light');
  await P.keyboard.press('Escape');
  // Mutation: ohne data-open-settings öffnet das Zahnrad nichts – die Prüfung erkennt es.
  await P.evaluate(() => document.querySelector('[data-v4-settings-button]').removeAttribute('data-open-settings'));
  await P.click('[data-v4-settings-button]');
  await P.waitForTimeout(100);
  const sMut = await settingsState();
  await P.evaluate(() => document.querySelector('[data-v4-settings-button]').setAttribute('data-open-settings', ''));
  check('TOP-2 Mutationsprobe: Zahnrad ohne Verdrahtung öffnet nichts (Prüfung schlägt an)', !sMut.open, JSON.stringify(sMut));

  // ---- TOP-3 Konto-Blatt: ganz oben „Einstellungen“, öffnet dieselben Einstellungen ----
  const accountState = () => P.evaluate(() => {
    const a = document.querySelector('[data-v4-account-panel]');
    const items = [...a.querySelectorAll('button, h2, section, select, input')].filter((e) => e.offsetParent !== null || e.getClientRects().length);
    const first = items.sort((x, y) => x.getBoundingClientRect().top - y.getBoundingClientRect().top || x.getBoundingClientRect().left - y.getBoundingClientRect().left)[0];
    return { open: !a.hidden, first: first ? first.textContent.trim() : null, sub: a.querySelector('[data-v4-account-sub]')?.textContent };
  });
  await P.click('[data-open-account]');
  await P.waitForTimeout(150);
  const a1 = await accountState();
  await P.click('[data-v4-account-panel] [data-v4-account-settings]');
  await P.waitForTimeout(150);
  const viaAcc = await settingsState();
  await P.keyboard.press('Escape');
  check('TOP-3 Avatar öffnet das Konto-Blatt; oberster Eintrag ist „Einstellungen“ und öffnet die Einstellungen', a1.open && a1.first === 'Einstellungen' && viaAcc.open, JSON.stringify({ a1, viaAcc }));
  await P.click('[data-open-account]');
  await P.waitForTimeout(100);
  await P.evaluate(() => document.querySelector('[data-v4-account-settings]').setAttribute('hidden', ''));
  const aMut = await accountState();
  await P.evaluate(() => document.querySelector('[data-v4-account-settings]').removeAttribute('hidden'));
  await P.keyboard.press('Escape');
  check('TOP-3 Mutationsprobe: ohne den Eintrag ist „Einstellungen“ nicht mehr oben (Prüfung schlägt an)', aMut.first !== 'Einstellungen', JSON.stringify(aMut));

  // ---- TOP-4 Zustandspunkt: grün nur, wenn die Quelle (sync-state.js) „ok“ meldet; ohne Quelle grau und ohne Text ----
  const dot = () => P.evaluate(() => {
    const b = document.querySelector('[data-v4-topbar] [data-v4-sync]');
    const probe = document.createElement('span'); probe.style.color = 'var(--success)'; b.append(probe);
    const green = getComputedStyle(probe).color; probe.remove();
    const t = b.querySelector('[data-sync-state-label]').textContent; // nur als Schlüssel ausgeben (keine sichtbaren Texte in results-*.json, Wächter-Ratchet)
    return { state: b.dataset.syncState ?? null, bg: getComputedStyle(b.querySelector('.shell-sync-dot')).backgroundColor, green, label: t ? (b.dataset.syncState === 'error' ? 'text:error' : 'text:warn') : '' };
  });
  const level = () => P.evaluate(async () => { const m = await import('/v4/shell/sync-state.js'); return m.currentSyncState()?.getOverallState?.()?.level ?? null; });
  const d0 = await dot();
  const lv0 = await level();
  const consistent = (d, lv) => (d.bg === d.green) === (lv === 'ok');
  // Isoliert: Kopfleiste ohne Quelle, mit 'unknown' und (Gegenprobe) mit 'ok'.
  const iso = await P.evaluate(async () => {
    const { createTopbar } = await import('/v4/frame/topbar.js');
    const out = {};
    for (const [k, src] of [['none', null], ['unknown', { subscribe: (fn) => { fn({ level: 'unknown' }); return () => {}; } }], ['ok', { subscribe: (fn) => { fn({ level: 'ok' }); return () => {}; } }]]) {
      const host = document.createElement('div'); host.className = 'v4f'; host.style.cssText = 'position:fixed;left:0;top:300px;width:600px;height:60px;';
      document.querySelector('[data-v4-frame]').after(host);
      const tb = createTopbar({ host, kernel: { identity: { instance_id: 'local' } }, syncState: src });
      const b = tb.element.querySelector('[data-v4-sync]');
      const probe = document.createElement('span'); probe.style.color = 'var(--success)'; b.append(probe);
      out[k] = { state: b.dataset.syncState ?? null, green: getComputedStyle(b.querySelector('.shell-sync-dot')).backgroundColor === getComputedStyle(probe).color, label: b.querySelector('[data-sync-state-label]').textContent ? 'text' : '' };
      tb.destroy(); host.remove();
    }
    return out;
  });
  check('TOP-4 Zustandspunkt nie grün ohne Quelle: ohne Quelle und bei „unknown“ grau ohne Text, nur „ok“ grün; im Rahmen grün ⇔ getOverallState().level === "ok"',
    !iso.none.green && !iso.none.state && !iso.none.label && !iso.unknown.green && !iso.unknown.label && iso.ok.green && consistent(d0, lv0), JSON.stringify({ iso, d0, lv0 }));
  if (STUB && (await P.evaluate(() => !!window.__lab.kernel.metrics?.get?.()?.collections))) {
    await P.evaluate(() => window.__lab.kernel.sim.setBacklog(3));
    await P.waitForTimeout(1300);
    const d1 = await dot(); const lv1 = await level();
    await P.evaluate(() => window.__lab.kernel.sim.setBacklog(0));
    await P.waitForTimeout(1300);
    const d2 = await dot(); const lv2 = await level();
    check('TOP-4 stub: Rückstand → nicht grün (Text aus der Quelle), Rückstand 0 → Punkt folgt der Quelle', d1.bg !== d1.green && consistent(d1, lv1) && consistent(d2, lv2), JSON.stringify({ d1, lv1, d2, lv2 }));
  }
  // Mutation: Kopfleiste mit Quelle „unknown“, Punkt von Hand auf „healthy“ gesetzt → die Prüfung (grün ⇔ ok) schlägt an.
  const isoMut = await P.evaluate(async () => {
    const { createTopbar } = await import('/v4/frame/topbar.js');
    const host = document.createElement('div'); host.className = 'v4f'; host.style.cssText = 'position:fixed;left:0;top:300px;width:600px;height:60px;';
    document.querySelector('[data-v4-frame]').after(host);
    const tb = createTopbar({ host, kernel: { identity: { instance_id: 'local' } }, syncState: { subscribe: (fn) => { fn({ level: 'unknown' }); return () => {}; } } });
    const b = tb.element.querySelector('[data-v4-sync]');
    b.dataset.syncState = 'healthy';
    const probe = document.createElement('span'); probe.style.color = 'var(--success)'; b.append(probe);
    const green = getComputedStyle(b.querySelector('.shell-sync-dot')).backgroundColor === getComputedStyle(probe).color;
    tb.destroy(); host.remove();
    return { green, level: 'unknown' };
  });
  check('TOP-4 Mutationsprobe: erzwungenes Grün bei Quelle „unknown“ fällt auf (grün ⇔ ok verletzt)', isoMut.green && isoMut.level !== 'ok', JSON.stringify(isoMut));

  // ---- Aufnahmen 1280/420 (Kopfleiste, Einstellungen, Konto) ----
  await P.screenshot({ path: shot('top-1280.png') });
  await P.click('[data-v4-settings-button]'); await P.waitForTimeout(150);
  await P.screenshot({ path: shot('top-settings-1280.png') });
  await P.keyboard.press('Escape');
  await P.click('[data-open-account]'); await P.waitForTimeout(150);
  await P.screenshot({ path: shot('top-account-1280.png') });
  await P.keyboard.press('Escape');
  await P.setViewportSize({ width: 420, height: 800 }); await P.waitForTimeout(300);
  const narrow = await P.evaluate(() => { const t = document.querySelector('[data-v4-topbar] .topbar'); const kids = [...t.querySelectorAll('.start-button, .shell-instance-button, [data-v4-sync], .account-button')].map((e) => e.getBoundingClientRect()); return { fits: t.scrollWidth <= t.clientWidth + 1, inside: kids.every((r) => r.left >= 0 && r.right <= innerWidth && r.height > 0) }; });
  await P.screenshot({ path: shot('top-420.png') });
  // FR6c: unter 560 px kein Zahnrad; Einstellungen über das Konto-Blatt (oberster Eintrag).
  const gear420 = await P.$eval('[data-v4-settings-button]', (b) => getComputedStyle(b).display);
  await P.click('[data-open-account]'); await P.waitForTimeout(150);
  await P.click('[data-v4-account-settings]'); await P.waitForTimeout(150);
  await P.screenshot({ path: shot('top-settings-420.png') });
  const s420 = await settingsState();
  await P.keyboard.press('Escape');
  check('TOP-5 (FR6c) 420 px: Kopfleiste ohne Überlauf, Bedienelemente sichtbar, Zahnrad ausgeblendet, Einstellungen über das Konto erreichbar und passen in die Breite', narrow.fits && narrow.inside && gear420 === 'none' && s420.open, JSON.stringify({ narrow, gear420, s420 }));
  void o2;
  check('TOP ohne Seitenfehler', perr.length === 0, perr.join(' | '));
  await ctx.close();
}

// ================= FRX: Rahmen-Befunde der Mängel-Abnahme 16.10. (M-02 Esc im Editor, M-11 Vollbild im Layout-Menü,
// Nebenbefund 420 px Kopfleiste). Eigene Seiten. FRX_ONLY=1 lässt nur diesen Block laufen. Echte Maus und Tastatur.
// M-02 nur gegen den Stub (die Attrappe hat kein kernel.source): Monaco (gebundene Shell, ?shell=bound) und der
// textarea-Rückfall (Rohpfade). M-11 und 420 px laufen in mock und stub.
async function frx() {
  const mclick = async (P, sel, { button = 'left' } = {}) => {
    const h = typeof sel === 'string' ? await P.$(sel) : sel;
    if (!h) return { ok: false, why: `fehlt ${sel}` };
    const r = await h.evaluate((el) => { const b = el.getBoundingClientRect(); const x = b.left + b.width / 2; const y = b.top + b.height / 2; const hit = document.elementFromPoint(x, y); return { x, y, hit: !!hit && (hit === el || el.contains(hit)) }; });
    if (!r.hit) return { ok: false, why: 'verdeckt' };
    await P.mouse.move(r.x, r.y, { steps: 3 });
    await P.mouse.click(r.x, r.y, { button });
    return { ok: true };
  };
  const newLab = async ({ bound = false, w = 1280 } = {}) => {
    const ctx = await browser.newContext({ viewport: { width: w, height: 800 }, locale: 'de-DE' });
    const P = await ctx.newPage();
    const perr = [];
    P.on('pageerror', (e) => perr.push(String(e)));
    P.on('console', (m) => { if (m.type() === 'error' && !/404|Failed to load resource|ctox:lib\/monaco/.test(m.text())) perr.push(m.text()); });
    // Gegenprobe ohne Fix (FRX_GEGENPROBE=1): dieselben Prüfungen gegen index.js/topbar.js/frame.css mit
    // herausgenommenem Fix (nur ausgelieferte Bytes, die Dateien bleiben unberührt). Erwartet: die FRX-Prüfungen fallen.
    if (process.env.FRX_GEGENPROBE) {
      const cut = { 'index.js': [["if (editorOverlayOpen(editorEl)) return;", 'return;'], ["if (layoutMenu && !layoutMenu.querySelector('[data-window-layout-control=\"fullscreen\"]'))", 'if (false)']],
        'topbar.js': [["visible = new Set([priority]);\n      items[priority].setAttribute('data-v4-tab-shrink', '');", 'visible.add(priority);']] };
      await P.route(/\/v4\/frame\/(index\.js|topbar\.js|frame\.css)(\?.*)?$/, async (route) => {
        const res = await route.fetch();
        let body = await res.text();
        const name = /(index\.js|topbar\.js|frame\.css)/.exec(route.request().url())[1];
        if (name === 'frame.css') body = body.replace(/\/\* ===== Kopfleiste: Überlauf[\s\S]*?(?=\/\* M-11: Symbol)/, '');
        for (const [a, b] of cut[name] || []) { if (!body.includes(a)) throw new Error(`Gegenprobe: ${name} ohne ${a}`); body = body.replace(a, b); }
        route.fulfill({ response: res, body });
      });
    }
    // Veröffentlichte Quelle von notes (M-02 seed) → neue Version; der Labor-Server kennt nur 1.0.0 (wie maengel.mjs).
    const notesDir = path.join(here, 'apps/notes/1.0.0');
    await P.route(/\/~a\/notes@(?!1\.0\.0)[^/]+\//, (route) => {
      const rest = new URL(route.request().url()).pathname.replace(/^.*\/~a\/notes@[^/]+\//, '');
      const file = path.join(notesDir, rest);
      if (!file.startsWith(notesDir) || !fs.existsSync(file)) { route.fulfill({ status: 404 }); return; }
      route.fulfill({ status: 200, contentType: rest.endsWith('.svg') ? 'image/svg+xml' : 'text/javascript', body: fs.readFileSync(file) });
    });
    // Lab-App mit langem Titel (Nebenbefund: „Zugangsdaten“).
    await P.route(/\/~a\/frxlong@/, (route) => route.fulfill({ status: 200, contentType: 'text/javascript', body: `export function mount(root) {
      root.innerHTML = '<div style="font:13px system-ui;padding:12px" data-frx-long>Zugangsdaten-Probe</div>';
      return { unmount() { root.replaceChildren(); }, snapshot() { return {}; } };
    }` }));
    await P.goto(`${base}/v4/frame/lab/${STUB ? `stub-lab.html?${bound ? 'shell=bound&' : ''}inst=frx-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}` : 'index.html'}`);
    await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
    const open = (id, g) => P.evaluate(async ([id, g]) => { const h = window.__lab.frame.shellHandle(); const o = h.openWindow(id, g ? { geometry: g } : {}); await h.win(o).host.ready; return o; }, [id, g]);
    return { ctx, P, perr, open };
  };
  const W = (id) => `.shell-window[data-window-app="${id}"]`;
  const mode = (P, id = 'notes') => P.$eval(W(id), (w) => w.dataset.v4Mode);

  // ---------------- M-02: Esc in der Code-Ansicht (nur stub) ----------------
  if (STUB) {
    const seed = (P) => P.evaluate(async (text) => { const src = window.__lab.kernel.source('notes'); await src.draft.write('index.js', text); return (await src.draft.publish({}).result)?.status; },
      fs.readFileSync(path.join(here, 'apps/notes/1.0.0/index.js'), 'utf8'));
    // ---- Monaco (gebundene Shell) ----
    {
      const { ctx, P, perr, open } = await newLab({ bound: true });
      const seeded = await seed(P);
      await open('notes', { x: 40, y: 30, w: 1100, h: 640 });
      await P.waitForTimeout(400);
      await P.keyboard.press('Control+Shift+KeyE');
      const hasMonaco = await P.waitForSelector(`${W('notes')} [data-v4-src-host] .v4-src-ed--monaco .view-lines`, { timeout: 10000 }).then(() => true, () => false);
      await P.waitForTimeout(600);
      const lines = `${W('notes')} [data-v4-src-host] .v4-src-ed--monaco .view-lines`;
      const edBox = async () => (await P.$(`${W('notes')} [data-v4-src-host] .v4-src-ed--monaco`)).boundingBox();
      const clickEditor = async () => { const b = await edBox(); await P.mouse.move(b.x + b.width / 2, b.y + 40, { steps: 3 }); await P.mouse.click(b.x + b.width / 2, b.y + 40); await P.waitForTimeout(150); };
      const backToCode = async () => { if ((await mode(P)) !== 'code') { await P.keyboard.press('Control+Shift+KeyE'); await P.waitForTimeout(300); } await clickEditor(); };
      const active = () => P.evaluate(() => `${document.activeElement?.tagName}.${String(document.activeElement?.className || '').split(' ')[0]}`);
      const text = () => P.$eval(lines, (e) => e.innerText);
      // a) Klick in den Editor, tippen, Esc → App; Entwurf bleibt
      await clickEditor();
      await P.keyboard.press('End');
      await P.keyboard.type(' /* frx-entwurf */');
      const focusA = await active();
      await P.screenshot({ path: shot('frx-m02-monaco-code-1280.png') });
      await P.keyboard.press('Escape');
      await P.waitForTimeout(250);
      const a = { mode: await mode(P), appVisible: await P.$eval(W('notes'), (w) => { const c = w.querySelector('.shell-window-content'); return w.querySelector('[data-v4-src-host]').hidden && !!c && c.checkVisibility() && c.getBoundingClientRect().width > 0 && !!c.firstElementChild; }) };
      await P.screenshot({ path: shot('frx-m02-monaco-esc-app-1280.png') });
      await P.keyboard.press('Control+Shift+KeyE');
      await P.waitForTimeout(300);
      const kept = (await text()).includes('frx-entwurf');
      check('FRX M-02 Monaco: Klick in den Editor, tippen, Esc → zurück in der App; Code-Ansicht wieder offen: Entwurf erhalten',
        hasMonaco && seeded === 'accepted' && focusA === 'TEXTAREA.inputarea' && a.mode === 'app' && a.appVisible && kept, JSON.stringify({ hasMonaco, seeded, focusA, a, kept }));
      // b) Vorschläge (Strg+Leertaste): erstes Esc schließt sie, Ansicht bleibt; zweites Esc → App
      await backToCode();
      await P.keyboard.press('End'); await P.keyboard.press('Enter'); await P.keyboard.type('docu');
      await P.keyboard.press('Control+Space');
      const sug = await P.waitForSelector(`${W('notes')} .suggest-widget.visible`, { timeout: 4000 }).then(() => true, () => false);
      await P.screenshot({ path: shot('frx-m02-monaco-suggest-1280.png') });
      await P.keyboard.press('Escape'); await P.waitForTimeout(250);
      const b1 = { mode: await mode(P), open: !!(await P.$(`${W('notes')} .suggest-widget.visible`)) };
      await P.keyboard.press('Escape'); await P.waitForTimeout(250);
      const b2 = await mode(P);
      check('FRX M-02 Monaco-Vorschläge offen: erstes Esc schließt nur die Vorschläge (Ansicht bleibt Code), zweites Esc → App',
        sug && b1.mode === 'code' && !b1.open && b2 === 'app', JSON.stringify({ sug, b1, b2 }));
      // c) Suchen (Cmd/Strg+F): Fokus im Suchfeld; erstes Esc schließt die Suche, zweites → App
      await backToCode();
      await P.keyboard.press(process.platform === 'darwin' ? 'Meta+KeyF' : 'Control+KeyF');
      const find = await P.waitForSelector(`${W('notes')} .find-widget.visible`, { timeout: 4000 }).then(() => true, () => false);
      const focusC = await active();
      await P.screenshot({ path: shot('frx-m02-monaco-find-1280.png') });
      await P.keyboard.press('Escape'); await P.waitForTimeout(250);
      const c1 = { mode: await mode(P), open: !!(await P.$(`${W('notes')} .find-widget.visible`)), focus: await active() };
      await P.keyboard.press('Escape'); await P.waitForTimeout(250);
      const c2 = await mode(P);
      check('FRX M-02 Monaco-Suche offen: erstes Esc schließt nur die Suche (Ansicht bleibt Code), zweites Esc → App',
        find && c1.mode === 'code' && !c1.open && c2 === 'app', JSON.stringify({ find, focusC, c1, c2 }));
      // d) Hover (Cmd/Strg+K, Cmd/Strg+I): erstes Esc schließt den Hover, zweites → App
      await backToCode();
      const mod = process.platform === 'darwin' ? 'Meta' : 'Control';
      await P.keyboard.press(`${mod}+KeyK`); await P.keyboard.press(`${mod}+KeyI`);
      const hov = await P.waitForSelector(`${W('notes')} .monaco-hover:not(.hidden)`, { timeout: 4000 }).then(() => true, () => false);
      await P.keyboard.press('Escape'); await P.waitForTimeout(250);
      const d1 = { mode: await mode(P), open: await P.$$eval(`${W('notes')} .monaco-hover:not(.hidden)`, (e) => e.filter((x) => x.checkVisibility()).length) };
      await P.keyboard.press('Escape'); await P.waitForTimeout(250);
      const d2 = await mode(P);
      check('FRX M-02 Monaco-Hover offen: erstes Esc schließt nur den Hover, zweites Esc → App', hov && d1.mode === 'code' && d1.open === 0 && d2 === 'app', JSON.stringify({ hov, d1, d2 }));
      // e) Kontextmenü (Rechtsklick in den Editor): erstes Esc schließt das Menü, zweites → App
      await backToCode();
      const b = await edBox();
      await P.mouse.click(b.x + b.width / 2, b.y + 40, { button: 'right' }); await P.waitForTimeout(400);
      const ctxOpen = await P.evaluate(() => [...document.querySelectorAll('.shadow-root-host')].some((h) => (h.shadowRoot?.querySelector('.monaco-menu')?.getBoundingClientRect().width || 0) > 0));
      await P.screenshot({ path: shot('frx-m02-monaco-kontextmenue-1280.png') });
      await P.keyboard.press('Escape'); await P.waitForTimeout(250);
      const e1 = { mode: await mode(P), open: await P.evaluate(() => [...document.querySelectorAll('.shadow-root-host')].some((h) => (h.shadowRoot?.querySelector('.monaco-menu')?.getBoundingClientRect().width || 0) > 0)) };
      await P.keyboard.press('Escape'); await P.waitForTimeout(250);
      const e2 = await mode(P);
      check('FRX M-02 Monaco-Kontextmenü offen: erstes Esc schließt nur das Menü, zweites Esc → App', ctxOpen && e1.mode === 'code' && !e1.open && e2 === 'app', JSON.stringify({ ctxOpen, e1, e2 }));
      // f) Nicht kaputt: Agent-Eingabe (Esc leert den Text, bleibt im Code), Layout-Menü (Esc schließt es, bleibt im
      //    Code, auch mit Fokus im Editor), Statuszone/App-Menü (Esc schließt die Liste, bleibt im Code)
      await backToCode();
      const agentIn = `${W('notes')} [data-v4-agent]:not([hidden]) [data-agent-input]`;
      const hasAgent = !!(await P.$(agentIn));
      let f1 = null;
      if (hasAgent) {
        await mclick(P, agentIn);
        await P.keyboard.type('Entwurf an den Agenten');
        await P.keyboard.press('Escape'); await P.waitForTimeout(200);
        f1 = { mode: await mode(P), value: await P.$eval(agentIn, (i) => i.value) };
      }
      await clickEditor();
      const trig = await (await P.$(`${W('notes')} [data-window-layout-trigger]`)).boundingBox();
      await P.mouse.move(trig.x - 30, trig.y + 30); await P.mouse.move(trig.x + trig.width / 2, trig.y + trig.height / 2, { steps: 5 });
      const menuOpen = await P.waitForSelector(`${W('notes')} [data-window-layout-menu]:not([hidden])`, { timeout: 3000 }).then(() => true, () => false);
      const focusF = await active();
      await P.keyboard.press('Escape'); await P.waitForTimeout(200);
      const f2 = { mode: await mode(P), menuHidden: await P.$eval(`${W('notes')} [data-window-layout-menu]`, (m) => m.hidden) };
      await P.mouse.move(600, 500, { steps: 3 });
      await mclick(P, `${W('notes')} .shell-window-v2-icon`);
      await P.waitForTimeout(200);
      const zoneOpen = !!(await P.$(`${W('notes')} .v4-status-zone__list:not([hidden])`));
      await P.keyboard.press('Escape'); await P.waitForTimeout(200);
      const f3 = { mode: await mode(P), listOpen: !!(await P.$(`${W('notes')} .v4-status-zone__list:not([hidden])`)) };
      check('FRX M-02 nicht kaputt: Esc in der Agent-Eingabe leert nur den Text, Esc bei offenem Layout-Menü (Fokus im Editor) schließt nur das Menü, Esc bei offenem App-Menü schließt nur die Liste – Ansicht bleibt jeweils Code',
        hasAgent && f1?.mode === 'code' && f1.value === '' && menuOpen && focusF === 'TEXTAREA.inputarea' && f2.mode === 'code' && f2.menuHidden && zoneOpen && f3.mode === 'code' && !f3.listOpen,
        JSON.stringify({ hasAgent, f1, menuOpen, focusF, f2, zoneOpen, f3 }));
      check('FRX M-02 Monaco ohne Seitenfehler', perr.length === 0, perr.join(' | '));
      await ctx.close();
    }
    // ---- textarea-Rückfall (Shell aus Rohpfaden: ctox:lib/monaco nicht auflösbar) ----
    {
      const { ctx, P, perr, open } = await newLab({ bound: false });
      const seeded = await seed(P);
      await open('notes', { x: 40, y: 30, w: 1100, h: 640 });
      await P.waitForTimeout(400);
      await P.keyboard.press('Control+Shift+KeyE');
      const ta = `${W('notes')} [data-v4-src-host] textarea.v4-src-ed__text`;
      const hasTa = await P.waitForSelector(ta, { timeout: 8000 }).then(() => true, () => false);
      let r = null;
      if (hasTa) {
        await mclick(P, ta);
        await P.keyboard.press('End');
        await P.keyboard.type(' /* frx-ta */');
        await P.keyboard.press('Escape'); await P.waitForTimeout(200);
        const m1 = await mode(P);
        await P.screenshot({ path: shot('frx-m02-textarea-esc-app-1280.png') });
        await P.keyboard.press('Control+Shift+KeyE'); await P.waitForTimeout(200);
        r = { m1, kept: (await P.$eval(ta, (t) => t.value)).includes('frx-ta'), monaco: !!(await P.$(`${W('notes')} .monaco-editor`)) };
      }
      check('FRX M-02 textarea-Rückfall: Klick in den Editor, tippen, Esc → App; Entwurf erhalten', seeded === 'accepted' && hasTa && r.m1 === 'app' && r.kept && !r.monaco, JSON.stringify({ seeded, hasTa, r }));
      check('FRX M-02 textarea ohne Seitenfehler', perr.length === 0, perr.join(' | '));
      await ctx.close();
    }
  }

  // ---------------- M-11: „Vollbild“ im Layout-Menü (mock + stub) ----------------
  {
    const { ctx, P, perr, open } = await newLab();
    await open('notes', { x: 360, y: 160, w: 560, h: 380 });
    await P.waitForTimeout(300);
    const trigSel = `${W('notes')} [data-window-layout-trigger]`;
    const hoverOpen = async () => {
      const t = await (await P.$(trigSel)).boundingBox();
      await P.mouse.move(t.x - 40, t.y + 40); await P.mouse.move(t.x + t.width / 2, t.y + t.height / 2, { steps: 6 });
      return P.waitForSelector(`${W('notes')} [data-window-layout-menu]:not([hidden])`, { timeout: 3000 }).then(() => true, () => false);
    };
    const item = () => P.$eval(`${W('notes')} [data-window-layout-menu]`, (m) => {
      const o = [...m.querySelectorAll('[data-window-layout-control]')];
      const fsb = m.querySelector('[data-window-layout-control="fullscreen"]');
      const min = m.querySelector('[data-window-layout-control="minimize"]');
      const fr = fsb?.getBoundingClientRect(); const mr = min?.getBoundingClientRect(); const box = m.getBoundingClientRect();
      return { order: o.map((b) => b.dataset.windowLayoutControl), label: fsb?.querySelector('.shell-window-layout-label')?.textContent, aria: fsb?.getAttribute('aria-label'), role: fsb?.getAttribute('role'),
        wide: fsb?.classList.contains('is-wide'), sameStyle: !!fsb && !!min && getComputedStyle(fsb).height === getComputedStyle(min).height && getComputedStyle(fsb).fontSize === getComputedStyle(min).fontSize && Math.abs(fr.width - mr.width) < 1,
        below: !!fr && !!mr && fr.top >= mr.bottom - 0.5 && fr.bottom <= box.bottom + 0.5, state: fsb?.dataset.fullscreen };
    });
    const opened = await hoverOpen();
    const i0 = await item();
    await P.screenshot({ path: shot('frx-m11-layout-menue-1280.png') });
    check('FRX M-11 Layout-Menü: letzter Eintrag „Vollbild“ unter „Minimieren“, gleiche Klassen/Rolle/Maße wie die breiten Einträge',
      opened && i0.order.at(-1) === 'fullscreen' && i0.order.at(-2) === 'minimize' && i0.label === 'Vollbild' && i0.aria === 'Vollbild' && i0.role === 'menuitem' && i0.wide && i0.sameStyle && i0.below, JSON.stringify(i0));
    // Tastatur: Enter am Knopf öffnet, Pfeil ab zu „Minimieren“, nochmals ab zu „Vollbild“, Pfeil auf zurück, Esc schließt
    await P.mouse.move(700, 700, { steps: 3 });
    await P.waitForTimeout(450);
    await P.focus(trigSel);
    await P.keyboard.press('Enter');
    const k = [];
    for (let n = 0; n < 6; n++) { await P.keyboard.press('ArrowDown'); k.push(await P.evaluate(() => document.activeElement?.dataset?.windowLayoutControl)); }
    await P.keyboard.press('ArrowUp');
    const up = await P.evaluate(() => document.activeElement?.dataset?.windowLayoutControl);
    await P.keyboard.press('Escape');
    const escHidden = await P.$eval(`${W('notes')} [data-window-layout-menu]`, (m) => m.hidden);
    check('FRX M-11 Tastatur: Pfeil ab erreicht „Vollbild“ nach „Minimieren“ und bleibt dort, Pfeil auf zurück zu „Minimieren“, Esc schließt',
      k.includes('fullscreen') && k[k.indexOf('fullscreen') - 1] === 'minimize' && k.at(-1) === 'fullscreen' && up === 'minimize' && escHidden, JSON.stringify({ k, up, escHidden }));
    // Spy: Aufrufe von requestFullscreen/exitFullscreen (falls headless verweigert)
    await P.evaluate(() => {
      window.__frxFs = [];
      const rq = Element.prototype.requestFullscreen; const ex = Document.prototype.exitFullscreen;
      Element.prototype.requestFullscreen = function (...a) { window.__frxFs.push(`request:${this.closest?.('.shell-window')?.dataset.windowApp || '?'}:${this.classList.contains('shell-window-content') ? 'content' : this.className}`); return rq.apply(this, a); };
      Document.prototype.exitFullscreen = function (...a) { window.__frxFs.push('exit'); return ex.apply(this, a); };
    });
    const fsState = () => P.evaluate(() => ({ inWin: !!document.fullscreenElement?.closest?.('.shell-window[data-window-app="notes"]'), el: document.fullscreenElement?.className?.toString().slice(0, 40) || null }));
    // Klick (echte Maus) auf „Vollbild“
    await hoverOpen();
    const c = await mclick(P, `${W('notes')} [data-window-layout-control="fullscreen"]`);
    await P.waitForTimeout(400);
    const s1 = await fsState();
    const i1 = await item();
    const menuHidden1 = await P.$eval(`${W('notes')} [data-window-layout-menu]`, (m) => m.hidden);
    await P.screenshot({ path: shot('frx-m11-vollbild-1280.png') });
    check('FRX M-11 Klick auf „Vollbild“ → Element-Vollbild im Fenster (Inhalt), Menü schließt, Eintrag heißt „Vollbild beenden“',
      c.ok && s1.inWin && menuHidden1 && i1.label === 'Vollbild beenden' && i1.aria === 'Vollbild beenden' && i1.state === 'on', JSON.stringify({ c, s1, menuHidden1, label: i1.label, spy: await P.evaluate(() => window.__frxFs) }));
    // Esc beendet den Vollbildmodus (Browser-Standard). Headless leitet die Taste nicht an die Browser-UI: dann wie der
    // Browser document.exitFullscreen() und das im Ergebnis vermerken.
    await P.keyboard.press('Escape');
    await P.waitForTimeout(400);
    let s2 = await fsState();
    let viaEsc = !s2.inWin;
    if (!viaEsc) { await P.evaluate(() => document.exitFullscreen()); await P.waitForTimeout(300); s2 = await fsState(); }
    const i2 = await item();
    check(`FRX M-11 Vollbild beenden (${viaEsc ? 'Esc' : 'headless: Esc erreicht die Browser-UI nicht, exitFullscreen() wie der Browser bei Esc'}) → Eintrag heißt wieder „Vollbild“`,
      !s2.inWin && i2.label === 'Vollbild' && i2.state === 'off', JSON.stringify({ viaEsc, s2, label: i2.label }));
    // Tastatur: Menü per Enter öffnen, zu „Vollbild“, Enter → Vollbild; nochmals über das Menü: „Vollbild beenden“ per Enter
    await P.focus(trigSel);
    await P.keyboard.press('Enter');
    for (let n = 0; n < 6; n++) await P.keyboard.press('ArrowDown');
    await P.keyboard.press('Enter');
    await P.waitForTimeout(400);
    const s3 = await fsState();
    const spy = await P.evaluate(() => window.__frxFs.slice());
    await P.evaluate(() => document.exitFullscreen?.().catch(() => {}));
    await P.waitForTimeout(300);
    check('FRX M-11 Tastatur: Enter auf „Vollbild“ → genau ein requestFullscreen am Fensterinhalt, Vollbild im Fenster',
      s3.inWin && spy.filter((x) => x.startsWith('request:notes:content')).length === 2 && spy.filter((x) => x.startsWith('request')).length === 2, JSON.stringify({ s3, spy }));
    check('FRX M-11 ohne Seitenfehler', perr.length === 0, perr.join(' | '));
    await ctx.close();
  }

  // ---------------- 420 px: langer Reiter + langer Workspace-Name (erweitert TOP-5) ----------------
  {
    const { ctx, P, perr, open } = await newLab({ w: 420 });
    const layout = { category: 'Labor', icon_svg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" fill="#0f766e"/></svg>', default_width: 360, default_height: 300 };
    await P.evaluate(async ([stub, layout]) => {
      if (stub) await window.__lab.kernel.sim.publishAppVersion('frxlong', '1.0.0', { title: 'Zugangsdaten', layout, styles: [] });
      else window.__lab.kernel.sim.addApp({ id: 'frxlong', title: 'Zugangsdaten', active_ver: '1.0.0', activated_ver: '1.0.0', state: 'activated', target_ver: null, error: null },
        { app_id: 'frxlong', app_ver: '1.0.0', entry: 'index.js', layout });
    }, [STUB, layout]);
    await P.waitForSelector('[data-v4-launcher] [data-v4-app="frxlong"]', { state: 'attached' });
    const LONG_WS = 'geo-credentials-1792128575178-ein-sehr-langer-workspace-name';
    const setWs = (name) => P.evaluate((n) => { document.querySelector('[data-v4-topbar] [data-shell-instance-name]').textContent = n; }, name);
    const measure = () => P.evaluate(() => {
      const top = document.querySelector('[data-v4-topbar] .topbar');
      const R = (e) => { if (!e) return null; const r = e.getBoundingClientRect(); return { l: Math.round(r.left * 10) / 10, r: Math.round(r.right * 10) / 10, w: Math.round(r.width) }; };
      const vis = [...document.querySelectorAll('[data-module-tabs] > .module-tab, [data-module-tabs] > .module-overflow')].filter((x) => x.checkVisibility());
      const tabs = vis.map(R);
      const name = document.querySelector('[data-v4-topbar] [data-shell-instance-name]');
      const left = document.querySelector('[data-v4-topbar] .topbar-left-group');
      const sync = R(document.querySelector('[data-v4-topbar] [data-v4-sync]'));
      const avatar = R(document.querySelector('[data-v4-topbar] .account-button'));
      const lefts = [...left.querySelectorAll('.start-button, .shell-instance-button')].map(R);
      const tabsRight = Math.max(...tabs.map((t) => t.r));
      const tabsLeft = Math.min(...tabs.map((t) => t.l));
      const focused = document.querySelector('[data-module-tabs] > .module-tab[data-running="focused"]');
      const label = focused?.querySelector('.module-tab-label');
      return { tabs, tabsRight, tabsLeft, sync, avatar, nameBox: R(name), nameClipped: name.scrollWidth > name.clientWidth + 1, leftRight: Math.max(...lefts.map((x) => x.r)),
        focusedVisible: !!focused?.checkVisibility(), labelClipped: !!label && label.scrollWidth > label.clientWidth + 1, labelText: label?.textContent, shrink: !!focused?.hasAttribute('data-v4-tab-shrink'),
        topFits: top.scrollWidth <= top.clientWidth + 1 && document.documentElement.scrollWidth <= innerWidth,
        ok: tabsRight <= Math.min(sync.l, avatar.l) + 0.5 && tabsLeft >= Math.max(...lefts.map((x) => x.r)) - 0.5 };
    });
    const variants = {};
    // 1) nur der lange Reiter (Workspace-Name des Labors)
    let longO = await open('frxlong');
    await P.waitForTimeout(400);
    variants.longTab = await measure();
    await P.screenshot({ path: shot('frx-420-langer-reiter.png'), clip: { x: 0, y: 0, width: 420, height: 50 } });
    // 2) langer Reiter + langer Workspace-Name
    await setWs(LONG_WS);
    await P.waitForTimeout(400);
    variants.longBoth = await measure();
    await P.screenshot({ path: shot('frx-420-langer-reiter-langer-workspace.png'), clip: { x: 0, y: 0, width: 420, height: 50 } });
    // 3) zwei Fenster (Notizen + Zugangsdaten vorn), langer Workspace-Name
    // Fenster nach vorn wie der Nutzer: Reiter klicken, liegt er im Überlauf, über „+N“.
    const focusO = async () => {
      const direct = await P.$('[data-module-tabs] > .module-tab[data-v4-tab-app="frxlong"]');
      if (direct) { await mclick(P, direct); } else { await mclick(P, '[data-v4-topbar] .module-overflow-trigger'); await P.waitForTimeout(150); await mclick(P, '[data-v4-topbar] .module-overflow-menu [data-v4-tab-app="frxlong"]'); }
      await P.waitForTimeout(150);
    };
    await open('notes');
    await focusO();
    await P.waitForTimeout(400);
    variants.twoWins = await measure();
    await P.screenshot({ path: shot('frx-420-zwei-fenster-langer-workspace.png'), clip: { x: 0, y: 0, width: 420, height: 50 } });
    // 4) ohne Fenster, langer Workspace-Name
    await P.evaluate(() => { const h = window.__lab.frame.shellHandle(); for (const w of h.windows()) h.closeWindow(w.ownerId); });
    await P.waitForTimeout(400);
    const nowin = await P.evaluate(() => { const top = document.querySelector('[data-v4-topbar] .topbar'); const n = document.querySelector('[data-shell-instance-name]').getBoundingClientRect(); const s = document.querySelector('[data-v4-topbar] [data-v4-sync]').getBoundingClientRect(); return { fits: top.scrollWidth <= top.clientWidth + 1, nameRight: Math.round(n.right), syncLeft: Math.round(s.left) }; });
    const vOk = (v) => v.ok && v.topFits && v.focusedVisible;
    check('FRX 420 px (TOP-5 erweitert): langer Reiter „Zugangsdaten“ allein, mit langem Workspace-Namen und mit zweitem Fenster – Reiter überlappen Punkt und Avatar nicht, kein waagrechter Überlauf, vorderer Reiter sichtbar (kürzt mit „…“, Workspace-Name kürzt mit „…“)',
      vOk(variants.longTab) && vOk(variants.longBoth) && vOk(variants.twoWins) && variants.longBoth.nameClipped && variants.twoWins.labelClipped && variants.twoWins.shrink && nowin.fits && nowin.nameRight <= nowin.syncLeft,
      JSON.stringify({ ...variants, nowin }));
    // Mutationsprobe: Fix per Stil neutralisiert (V2-Maße max-content, Name ungekürzt) → die Prüfung findet die Überlappung
    longO = await open('frxlong'); await open('notes');
    await focusO();
    await P.addStyleTag({ content: ':where([data-v4-topbar]) .topbar .module-tabs .module-tab[data-v4-tab-shrink] { flex: 0 0 auto !important; min-width: max-content !important; overflow: visible !important; } :where([data-v4-topbar]) .topbar .shell-instance-button [data-shell-instance-name] { max-width: none !important; overflow: visible !important; }' });
    await P.waitForTimeout(400);
    const mut = await measure();
    await P.screenshot({ path: shot('frx-420-mutation-ohne-fix.png'), clip: { x: 0, y: 0, width: 420, height: 50 } });
    check('FRX 420 px Mutationsprobe: ohne Schrumpfen/Kürzen überlappen Reiter Punkt bzw. Avatar – die Prüfung findet es', !mut.ok, JSON.stringify({ tabsRight: mut.tabsRight, tabsLeft: mut.tabsLeft, sync: mut.sync, avatar: mut.avatar, leftRight: mut.leftRight }));
    check('FRX 420 px ohne Seitenfehler', perr.length === 0, perr.join(' | '));
    await ctx.close();
  }
}

try {
  if (process.env.FRX_ONLY) { await frx(); throw Object.assign(new Error('frx-only'), { fr6bOnly: true }); }
  if (process.env.FR6C_ONLY) { await fr6c(); throw Object.assign(new Error('fr6c-only'), { fr6bOnly: true }); }
  if (process.env.TOP_ONLY) { await topbarLab(); throw Object.assign(new Error('top-only'), { fr6bOnly: true }); }
  // FR6b: FR6B_ONLY=1 → nur der FR6b-Block (Mutationsläufe).
  if (process.env.FR6B_ONLY) { await fr6b(); throw Object.assign(new Error('fr6b-only'), { fr6bOnly: true }); }
  if (process.env.Z45_ONLY) { await z45(); throw Object.assign(new Error('z45-only'), { fr6bOnly: true }); }
  if (process.env.ZONE2_ONLY) { await zone2(); throw Object.assign(new Error('zone2-only'), { fr6bOnly: true }); }
  if (process.env.MOBILE_ONLY) { await mobile(); throw Object.assign(new Error('mobile-only'), { fr6bOnly: true }); }
  if (process.env.ICON25_ONLY) { await icon25(); throw Object.assign(new Error('icon25-only'), { fr6bOnly: true }); }
  if (process.env.M09_ONLY) { await m09(); throw Object.assign(new Error('m09-only'), { fr6bOnly: true }); }
  if (process.env.AG_ONLY) { if (STUB) await ag(); throw Object.assign(new Error('ag-only'), { fr6bOnly: true }); }
  if (process.env.LOOP_ONLY) { if (STUB) await loop(); throw Object.assign(new Error('loop-only'), { fr6bOnly: true }); }
  await page.goto(`${base}/v4/frame/lab/${STUB ? 'stub-lab' : 'index'}.html`);
  await page.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
  await page.evaluate(() => { window.__noReload = 'marker'; });

  const tileIds = async () => page.$$eval('[data-v4-launcher] [data-v4-app]', (els) => els.map((e) => e.dataset.v4App).sort());
  await page.waitForFunction(() => document.querySelectorAll('[data-v4-launcher] [data-v4-app] img').length === 3);
  check('Launcher zeigt die 3 Apps aus kernel.apps', JSON.stringify(await tileIds()) === JSON.stringify(['ledger', 'notes', 'tasks']), (await tileIds()).join(','));
  const iconSrc = await page.$eval('[data-v4-app="notes"] img', (img) => img.getAttribute('src'));
  // Form von kernel.artifacts.url ohne Bindung (§16.39 Nr. 1, K-1d): aktive Shell des Kernels, nicht die des Rahmens.
  const activeShell = await page.evaluate(() => window.__lab.activeShell());
  check(STUB ? 'Launcher-Icon aus kernel.apps layout.icon_asset (versionierte URL)' : 'Icon ueber versionierte URL', !!activeShell && iconSrc === `/~a/notes@1.0.0~shell@${activeShell}/icon.svg`, iconSrc);
  check(STUB ? 'Kategorie-Gruppen aus kernel.apps layout.category' : 'Kategorie-Gruppen aus Manifest-layout', (await page.$$eval('[data-v4-group]', (g) => g.map((x) => x.dataset.v4Group))).join(',') === 'Arbeit,Finanzen');
  check('Keine Monogramme, wenn layout ein Icon hat', (await page.$$('[data-v4-app] .v4f-icon.is-glyph')).length === 0);
  if (STUB) {
    const iconOk = await page.evaluate(async () => {
      const img = document.querySelector('[data-v4-app="notes"] img');
      if (!img.complete) await new Promise((r) => img.addEventListener('load', r, { once: true }));
      return img.naturalWidth > 0;
    });
    check('Layout-Icon tatsaechlich geladen (naturalWidth > 0)', iconOk);
  }

  // Suche
  await startOpen(page);
  await page.fill('[data-v4-search]', 'buch');
  check('Suche filtert', JSON.stringify(await tileIds()) === JSON.stringify(['ledger']));
  await page.fill('[data-v4-search]', 'zzz');
  const emptyText = await page.$eval('[data-v4-launcher-body]', (b) => b.innerText.trim());
  check('Leere Suche zeigt keinen Platzhaltertext', emptyText === '', JSON.stringify(emptyText));
  await page.fill('[data-v4-search]', '');
  // Pin setzen
  await page.hover('[data-v4-app="notes"]');
  await page.click('[data-v4-app="notes"] [data-v4-pin]');
  // FR6c: angeheftete App = Reiter oben (Shell-V2: Reiter ordnen sich im nächsten Frame).
  await page.waitForSelector('[data-module-tabs] .module-tab[data-v4-pin="notes"]', { timeout: 3000 }).catch(() => {});
  check('Pin erscheint in der Reiterleiste', !!(await page.$('[data-module-tabs] .module-tab[data-v4-pin="notes"]')));

  // 3 Fenster oeffnen
  for (const id of ['notes', 'tasks', 'ledger']) {
    // Oeffnen ueber Suche + Enter (Kacheln koennen schon von Fenstern verdeckt sein).
    await startOpen(page);
    await page.fill('[data-v4-search]', id === 'notes' ? 'Notiz' : id === 'tasks' ? 'Aufg' : 'Buch');
    await page.press('[data-v4-search]', 'Enter'); // FR6d: Start schließt das Menü (Shell-V2)
    await page.waitForSelector(`.shell-window[data-window-app="${id}"] [data-lab-app="${id}"]`);
  }
  await page.waitForTimeout(700); // Morph-Animation des window-manager
  check('3 Fenster offen und App montiert', (await page.$$('.shell-window')).length === 3);
  check('Taskleiste zeigt 3 Fenster', (await page.$$('[data-v4-win]')).length === 3);
  check('Fenstertitel aus api.shell.setTitle', (await page.$eval('.shell-window[data-window-app="tasks"] .shell-window-title-text', (e) => e.textContent)) === 'Aufgaben');
  if (STUB) {
    const sz = await page.$eval('.shell-window[data-window-app="tasks"]', (e) => ({ w: Math.round(e.getBoundingClientRect().width), h: Math.round(e.getBoundingClientRect().height) }));
    check('Fenstergroesse aus layout.default_width/height', sz.w === 700 && sz.h === 480, JSON.stringify(sz));
  }
  // registerHeader(el) nach §16.19 Nr. 5: Element in den Kopf-Slot, neues ersetzt altes, null entfernt, sonst invalid.
  const hdrRes = await page.evaluate(() => {
    const rec = window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'tasks');
    const api = rec.host.instance.api;
    const slot = rec.handle.element.querySelector('[data-window-actions]');
    const a = document.createElement('button'); a.textContent = 'A'; a.dataset.hdr = 'a';
    const b = document.createElement('button'); b.textContent = 'B'; b.dataset.hdr = 'b';
    const r = {};
    r.ret = api.shell.registerHeader(a);
    r.afterA = [...slot.querySelectorAll('[data-hdr]')].map((x) => x.dataset.hdr).join(',');
    r.sameRef = slot.querySelector('[data-hdr="a"]') === a;
    api.shell.registerHeader(b);
    r.afterB = [...slot.querySelectorAll('[data-hdr]')].map((x) => x.dataset.hdr).join(',');
    try { api.shell.registerHeader({ kind: 'spec' }); r.invalid = 'kein Fehler'; } catch (e) { r.invalid = `${e.name}:${e.code}`; }
    api.shell.registerHeader(null);
    r.afterNull = slot.querySelectorAll('[data-hdr]').length;
    return r;
  });
  check('registerHeader(el): Referenz im Kopf-Slot, ersetzt, null entfernt, Nicht-Element invalid',
    hdrRes.ret === undefined && hdrRes.afterA === 'a' && hdrRes.sameRef && hdrRes.afterB === 'b' && hdrRes.invalid === 'CtoxError:invalid' && hdrRes.afterNull === 0, JSON.stringify(hdrRes));

  // ---------- FR5-1: api.shell.dialog als Blatt IM App-Fenster (nie Shell-Overlay), modal nur für dieses Fenster ----------
  const fr5Front = await page.evaluate(() => document.querySelector('.shell-window.is-focused')?.dataset.windowApp || null);
  if (!process.env.FR5_SKIP) {
    const W = '.shell-window[data-window-app="tasks"]';
    const startDialog = (spec) => page.evaluate((sp) => {
      const rec = window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'tasks');
      window.__dlg = { done: false, value: undefined };
      rec.host.instance.api.shell.dialog(sp).then((v) => { window.__dlg = { done: true, value: v }; });
    }, spec);
    const dlg = () => page.evaluate(() => window.__dlg);
    const geo = async () => page.evaluate((w) => {
      const win = document.querySelector(w);
      const sheets = [...document.querySelectorAll('[data-v4-sheet]')];
      const s = sheets[0];
      const wr = win.getBoundingClientRect();
      const sr = s?.getBoundingClientRect();
      const content = win.querySelector('[data-window-content]');
      const other = document.querySelector('.shell-window[data-window-app="notes"] [data-window-content]');
      return {
        n: sheets.length,
        inWindow: !!s && s.closest('.shell-window') === win,
        inside: !!sr && sr.left >= wr.left - 1 && sr.right <= wr.right + 1 && sr.top >= wr.top && sr.bottom <= wr.bottom + 1,
        // FR6a: keine Titelleiste; das Blatt hält die reservierte Zone frei, Knopfcluster bleibt bedienbar.
        belowTitle: !!sr && sr.top >= wr.top + parseFloat(getComputedStyle(win).getPropertyValue('--shell-zone-height')) - 1
          && (() => { const c = win.querySelector('[data-window-controls]').getBoundingClientRect(); return win.querySelector('[data-window-controls]').contains(document.elementFromPoint(c.left + c.width / 2, c.top + c.height / 2)); })(),
        front: win.classList.contains('is-focused'),
        contentInert: !!content?.inert,
        otherInert: !!other?.inert,
        focusInSheet: !!s && s.contains(document.activeElement),
        ok: s?.querySelector('[data-v4-sheet-ok]')?.textContent,
        okDanger: !!s?.querySelector('[data-v4-sheet-ok].is-danger'),
        cancel: s?.querySelector('[data-v4-sheet-cancel]')?.textContent,
        input: s?.querySelector('[data-v4-sheet-input]')?.value,
      };
    }, W);
    await startDialog({ kind: 'confirm', title: 'Kampagne löschen', text: 'Die Kampagne „Herbst“ wirklich löschen?', danger: true, default: false });
    await page.waitForSelector(`${W} [data-v4-sheet]`);
    const g1 = await geo();
    await page.screenshot({ path: shot('fr5-sheet-confirm-1280.png') });
    let trapped = true;
    for (let i = 0; i < 5; i++) { await page.keyboard.press('Tab'); trapped = trapped && await page.evaluate(() => !!document.activeElement?.closest('[data-v4-sheet]')); }
    await page.keyboard.press('Shift+Tab');
    trapped = trapped && await page.evaluate(() => !!document.activeElement?.closest('[data-v4-sheet]'));
    await page.focus(`${W} [data-v4-sheet-ok]`);
    await page.keyboard.press('Enter');
    await page.waitForFunction(() => window.__dlg.done);
    const r1 = await dlg();
    const after1 = await geo();
    check('FR5-1 confirm: Blatt im App-Fenster (Fenster nach vorn, Reservezone + Knöpfe frei), Inhalt inert, anderes Fenster frei, deutsch + destruktiv rot',
      g1.n === 1 && g1.inWindow && g1.inside && g1.belowTitle && g1.front && g1.contentInert && !g1.otherInert && g1.focusInSheet && g1.ok === 'Löschen' && g1.okDanger && g1.cancel === 'Abbrechen', JSON.stringify(g1));
    check('FR5-1 Fokusfalle: Tab/Shift+Tab bleiben im Blatt', trapped);
    check('FR5-1 Enter bestätigt (true), Blatt weg, Inhalt wieder bedienbar', r1.value === true && after1.n === 0 && !after1.contentInert, JSON.stringify({ r1, after1 }));
    await startDialog({ kind: 'confirm', title: 'Löschen', text: 'Wirklich?', default: false });
    await page.waitForSelector(`${W} [data-v4-sheet]`);
    await page.keyboard.press('Escape');
    await page.waitForFunction(() => window.__dlg.done);
    check('FR5-1 Esc bricht ab (confirm → false)', (await dlg()).value === false);
    await startDialog({ kind: 'prompt', title: 'Neues Notizbuch', text: 'Name des Notizbuchs', default: 'Alt', confirm_label: 'Anlegen' });
    await page.waitForSelector(`${W} [data-v4-sheet-input]`);
    const gp = await geo();
    await page.keyboard.type('Projekt X'); // Startwert ist markiert → wird ersetzt
    await page.keyboard.press('Enter');
    await page.waitForFunction(() => window.__dlg.done);
    const rp = await dlg();
    await startDialog({ kind: 'prompt', title: 'Name', default: null });
    await page.waitForSelector(`${W} [data-v4-sheet-input]`);
    await page.keyboard.type('weg');
    await page.keyboard.press('Escape');
    await page.waitForFunction(() => window.__dlg.done);
    const rpEsc = await dlg();
    check('FR5-1 prompt: Eingabefeld mit Startwert, Enter liefert Text, Esc liefert null', gp.input === 'Alt' && gp.ok === 'Anlegen' && gp.focusInSheet && rp.value === 'Projekt X' && rpEsc.value === null, JSON.stringify({ gp, rp, rpEsc }));
    await startDialog({ kind: 'select', title: 'Priorität', options: [{ value: 'low', label: 'Niedrig' }, { value: 'mid', label: 'Mittel' }, { value: 'high', label: 'Hoch' }], default: 'mid' });
    await page.waitForSelector(`${W} [data-v4-sheet-options]`);
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await page.waitForFunction(() => window.__dlg.done);
    const rs = await dlg();
    check('FR5-1 Auswahl: vorgewählt, Pfeiltaste wechselt, Enter liefert den Wert', rs.value === 'high', JSON.stringify(rs));
    const queued = await page.evaluate(async () => {
      const api = window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'tasks').host.instance.api;
      const out = [];
      const a = api.shell.dialog({ kind: 'confirm', title: 'Erste' }).then((v) => out.push(['a', v]));
      const b = api.shell.dialog({ kind: 'alert', title: 'Zweite' }).then((v) => out.push(['b', v]));
      await new Promise((r) => setTimeout(r, 50));
      const first = [...document.querySelectorAll('[data-v4-sheet] .v4-sheet-title')].map((e) => e.textContent).join('|');
      document.querySelector('[data-v4-sheet-cancel]').click();
      await new Promise((r) => setTimeout(r, 50));
      const second = [...document.querySelectorAll('[data-v4-sheet] .v4-sheet-title')].map((e) => e.textContent).join('|');
      document.querySelector('[data-v4-sheet-ok]').click();
      await Promise.all([a, b]);
      return { first, second, out };
    });
    check('FR5-1 Reihenfolge: zweiter Dialog erst nach dem ersten, alert → true',
      queued.first === 'Erste' && queued.second === 'Zweite' && JSON.stringify(queued.out) === '[["a",false],["b",true]]', JSON.stringify(queued));
  }

  // ---------- FR5-2: generisches Kontextmenü im Fenster (data-context-*) + „CTOX fragen…“ ----------
  if (!process.env.FR5_SKIP && !process.env.FR5_SKIP2) {
    const W = '.shell-window[data-window-app="tasks"]';
    await page.evaluate(() => {
      const rec = window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'tasks');
      const appRoot = rec.host.instance.root;
      const box = document.createElement('div');
      box.dataset.fr5 = '';
      box.innerHTML = `
        <div data-fr5-full tabindex="0" data-context-record-id="t-7" data-context-record-type="task" data-context-label="Aufgabe 7" data-context-actions="open rename delete" style="padding:4px">Aufgabe 7</div>
        <div data-fr5-bare data-context-record-id="t-8" data-context-record-type="task" style="padding:4px">Aufgabe 8</div>
        <div data-fr5-own data-context-record-id="t-9" style="padding:4px">Eigenes Menü</div>`;
      appRoot.querySelector('[data-lab-app]').prepend(box);
      window.__ctxEv = [];
      box.addEventListener('ctox:context-action', (ev) => window.__ctxEv.push(ev.detail));
      box.querySelector('[data-fr5-own]').addEventListener('contextmenu', (ev) => ev.preventDefault());
      window.__cmds = [];
      window.__fr5Command = rec.host.instance.api.command;
      rec.host.instance.api.command = (type, payload, opts) => { window.__cmds.push({ type, payload, opts }); return { id: 'cmd-fr5' }; };
    });
    const menuInfo = () => page.evaluate((w) => {
      const m = [...document.querySelectorAll('[data-v4-menu]')];
      const win = document.querySelector(w);
      return {
        n: m.length,
        inWindow: !!m[0] && m[0].closest('.shell-window') === win,
        labels: m[0] ? [...m[0].querySelectorAll('[data-v4-menu-item]')].map((b) => b.textContent) : [],
        focus: m[0]?.contains(document.activeElement) || false,
      };
    }, W);
    await page.click(`${W} [data-fr5-full]`, { button: 'right' });
    await page.waitForSelector(`${W} [data-v4-menu]`);
    const m1 = await menuInfo();
    await page.screenshot({ path: shot('fr5-context-menu-1280.png') });
    await page.click(`${W} [data-v4-menu-item="record.delete"]`);
    const ev1 = await page.evaluate(() => window.__ctxEv.slice());
    check('FR5-2 Rechtsklick: Menü im Fenster mit den angebotenen Standardeinträgen + „CTOX fragen…“',
      m1.n === 1 && m1.inWindow && m1.focus && JSON.stringify(m1.labels) === JSON.stringify(['Öffnen', 'Umbenennen', 'Löschen', 'CTOX fragen…']), JSON.stringify(m1));
    check('FR5-2 Wahl „Löschen“ → ctox:context-action {action, record} am Datensatz', JSON.stringify(ev1) === JSON.stringify([{ action: 'delete', record: { id: 't-7', type: 'task', label: 'Aufgabe 7' } }]), JSON.stringify(ev1));
    await page.click(`${W} [data-fr5-bare]`, { button: 'right' });
    await page.waitForSelector(`${W} [data-v4-menu]`);
    const m2 = await menuInfo();
    await page.keyboard.press('Escape');
    const m2closed = (await menuInfo()).n === 0;
    check('FR5-2 ohne data-context-actions nur „CTOX fragen…“; Esc schließt', JSON.stringify(m2.labels) === '["CTOX fragen…"]' && m2closed, JSON.stringify(m2));
    await page.click(`${W} [data-fr5-own]`, { button: 'right' });
    await page.waitForTimeout(80);
    const own = (await menuInfo()).n;
    await page.click(`${W} [data-lab-input]`, { button: 'right' });
    await page.waitForTimeout(80);
    const inInput = (await menuInfo()).n;
    await page.keyboard.press('Escape');
    check('FR5-2 App-eigenes Menü (preventDefault) und Eingabefeld ohne Datensatz: Rahmen bleibt still', own === 0 && inInput === 0, `${own}/${inInput}`);
    await page.focus(`${W} [data-fr5-full]`);
    await page.keyboard.press('Shift+F10');
    await page.waitForSelector(`${W} [data-v4-menu]`);
    const mk = await menuInfo();
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    const ev2 = await page.evaluate(() => window.__ctxEv.slice(1));
    check('FR5-2 Tastatur Shift+F10: Menü am fokussierten Datensatz, Pfeil + Enter wählt („Umbenennen“)',
      mk.n === 1 && mk.focus && ev2.length === 1 && ev2[0].action === 'rename', JSON.stringify({ mk, ev2 }));
    await page.click(`${W} [data-fr5-full]`, { button: 'right' });
    await page.waitForSelector(`${W} [data-v4-menu]`);
    await page.click(`${W} [data-v4-menu-item="ctox.ask"]`);
    await page.waitForSelector(`${W} [data-v4-sheet-input]`);
    const askTitle = await page.$eval(`${W} [data-v4-sheet] .v4-sheet-title`, (e) => e.textContent);
    await page.keyboard.type('Was ist hier offen?');
    await page.keyboard.press('Enter');
    await page.waitForFunction(() => window.__cmds.length === 1);
    const cmd = await page.evaluate(() => window.__cmds[0]);
    check('FR5-2 „CTOX fragen…“ → Eingabeblatt → Standardbefehl ctox.chat.task mit Datensatzbezug',
      askTitle === 'CTOX fragen' && cmd.type === 'ctox.chat.task' && cmd.payload.source_app === 'tasks' && cmd.payload.record_ref === 't-7' && cmd.payload.text === 'Was ist hier offen?' && cmd.opts?.record_id === 't-7', JSON.stringify(cmd));
    await page.evaluate((front) => {
      document.querySelector('[data-fr5]')?.remove();
      window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'tasks').host.instance.api.command = window.__fr5Command;
      if (front) window.__lab.frame.shellHandle().navigate(front); // Fensterreihenfolge wie vor FR5 (Blatt holt sein Fenster nach vorn)
    }, fr5Front);
    await page.waitForTimeout(100);
  }

  // Geometrie veraendern: notes ziehen, tasks rechts anheften
  const hdr = await page.$('.shell-window[data-window-app="notes"] .shell-window-header');
  const hb = await hdr.boundingBox();
  await page.mouse.move(hb.x + 120, hb.y + 18);
  await page.mouse.down();
  await page.mouse.move(hb.x - 160, hb.y - 60, { steps: 8 });
  await page.mouse.move(hb.x - 200, hb.y - 90, { steps: 4 });
  await page.mouse.up();
  await page.evaluate(() => {
    const el = document.querySelector('.shell-window[data-window-app="tasks"]');
    el.querySelector('[data-window-layout-trigger]').click();
    el.querySelector('[data-window-layout-control="right"]').click();
  });
  await page.waitForTimeout(300);
  await page.click('.shell-window[data-window-app="notes"] [data-lab-input]');
  await page.fill('.shell-window[data-window-app="notes"] [data-lab-input]', 'halb ausgefuellt');

  const geom = () => page.$$eval('.shell-window', (els) => Object.fromEntries(els.map((e) => {
    const r = e.getBoundingClientRect();
    return [e.dataset.windowApp, { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height), z: Number(e.style.zIndex), focused: e.classList.contains('is-focused'), snap: e.dataset.snapZone || '' }];
  })));
  const before = await geom();
  await page.screenshot({ path: shot('frame-1280.png') });

  // Shell-Tausch stopShell -> startShell
  const swap = await page.evaluate(async () => {
    const r = await window.__lab.swapShell();
    return { ms: r.ms, windows: r.snap?.windows?.length, snapBytes: JSON.stringify(r.snap || null).length, swapped: r.swapped !== false, error: r.error || '' };
  });
  await page.waitForSelector('.shell-window[data-window-app="notes"] [data-lab-input]');
  await page.waitForTimeout(150);
  const after = await geom();
  const same = ['notes', 'tasks', 'ledger'].every((id) => before[id] && after[id]
    && Math.abs(before[id].x - after[id].x) <= 1 && Math.abs(before[id].y - after[id].y) <= 1
    && Math.abs(before[id].w - after[id].w) <= 1 && Math.abs(before[id].h - after[id].h) <= 1
    && before[id].snap === after[id].snap);
  check('Shell-Tausch: Fenster an gleicher Stelle/Groesse', same, JSON.stringify({ before, after }));
  const order = (g) => Object.entries(g).sort((a, b) => a[1].z - b[1].z).map(([k]) => k).join(',');
  check('Shell-Tausch: Fokus-Reihenfolge erhalten', order(before) === order(after), `${order(before)} / ${order(after)}`);
  check('Shell-Tausch: fokussiertes Fenster erhalten', Object.keys(before).find((k) => before[k].focused) === Object.keys(after).find((k) => after[k].focused));
  check('Shell-Tausch: App-Snapshot (Formular) erhalten', (await page.inputValue('.shell-window[data-window-app="notes"] [data-lab-input]')) === 'halb ausgefuellt');
  check('Shell-Tausch: Pin erhalten', !!(await page.$('[data-v4-pin="notes"]')));
  // E48: shell_preferences liest nur eine Shell-Version mit veröffentlichter Deklaration – shell@dev (stub-lab.html).
  const prefRow = await page.evaluate((stub) => (stub
    ? window.__lab.kernel.appApi('shell', 'dev').get('shell_preferences', 'owner').get()
    : window.__lab.kernel.sim.prefs.get('shell_preferences')), STUB);
  check('Pin steht in shell_preferences (eine Zeile, id = Nutzer)', prefRow?.id === 'owner' && prefRow?.taskbar_pins?.pins?.includes('notes'), JSON.stringify(prefRow));
  check('Shell-Tausch: genau ein Rahmen, ein Satz Shell-CSS (Theme, base, Shell-V2-Chrome, Rahmen, Shell-V2-Kopfleiste, Shell-V2-Desktop/Startmenü/Crew)', (await page.$$('[data-v4-frame]')).length === 1 && (await page.$$('link[data-v4-frame-css]')).length === 6);
  if (!STUB) check('Shell-Tausch: Kernel-Abos nicht verdoppelt', (await page.evaluate(() => window.__lab.kernel.sim.subscriberCount())) === 2);
  check('Shell-Tausch ohne Rueckfall', swap.swapped, swap.error);
  check('Shell-Tausch <= 2 s', swap.ms <= 2000, `${swap.ms.toFixed(1)} ms, Snapshot ${swap.snapBytes} B`);
  check('Fenstertitel ohne Versionsnummer', (await page.$$eval('.shell-window-title-text', (els) => els.map((e) => e.textContent))).every((t) => !/\d+\.\d+/.test(t)));
  check('Zonen-Platz im Chrome, ohne Eintraege 0 px', await page.$$eval('.shell-window', (els) => els.length === 3 && els.every((w) => {
    const slot = w.querySelector(':scope > [data-v4-zone-slot]');
    return slot && slot.getBoundingClientRect().height === 0 && !w.querySelector('[data-window-content] [data-v4-zone-slot]');
  })));

  // --- Doppelpuffer: Shell-Tausch mit 3 Fenstern ohne leeren Frame (MutationObserver + rAF + Pixelprobe) ---
  const probeSetup = () => page.evaluate(() => {
    const host = document.getElementById('ctox-shell');
    const visibleWithContent = () => {
      let best = 0;
      for (const root of host.querySelectorAll(':scope > [data-v4-frame]')) {
        if (!root.isConnected || getComputedStyle(root).opacity === '0') continue;
        const n = [...root.querySelectorAll('.shell-window')].filter((w) => w.style.display !== 'none' && w.querySelector('.v4-app-root')?.childElementCount > 0).length;
        best = Math.max(best, n);
      }
      return best;
    };
    const p = { samples: 0, min: Infinity, rafSamples: 0, rafMin: Infinity, running: true };
    const mo = new MutationObserver(() => { p.samples++; p.min = Math.min(p.min, visibleWithContent()); });
    mo.observe(host, { childList: true, subtree: true, attributes: true, attributeFilter: ['style', 'data-v4-retiring', 'data-v4-building'] });
    const raf = () => { if (!p.running) return; p.rafSamples++; p.rafMin = Math.min(p.rafMin, visibleWithContent()); requestAnimationFrame(raf); };
    requestAnimationFrame(raf);
    window.__probe = { p, stop() { p.running = false; mo.disconnect(); return { ...p }; } };
  });
  const pixelSame = (a, b) => page.evaluate(async ([a, b]) => {
    const img = async (b64) => createImageBitmap(await (await fetch(`data:image/png;base64,${b64}`)).blob());
    const [ia, ib] = await Promise.all([img(a), img(b)]);
    const c = new OffscreenCanvas(ia.width, ia.height);
    const g = c.getContext('2d', { willReadFrequently: true });
    g.drawImage(ia, 0, 0); const da = g.getImageData(0, 0, ia.width, ia.height).data;
    g.clearRect(0, 0, ia.width, ia.height); g.drawImage(ib, 0, 0); const db = g.getImageData(0, 0, ia.width, ia.height).data;
    let same = 0;
    for (let i = 0; i < da.length; i += 4) if (Math.abs(da[i] - db[i]) + Math.abs(da[i + 1] - db[i + 1]) + Math.abs(da[i + 2] - db[i + 2]) < 24) same++;
    return same / (da.length / 4);
  }, [a.toString('base64'), b.toString('base64')]);
  await page.focus('[data-v4-search]');
  await page.waitForTimeout(200);
  const shotBefore = await page.screenshot();
  await probeSetup();
  await page.evaluate(() => { window.__labMountDelayMs = 600; window.__swapDone = window.__lab.swapShell().then((r) => { window.__swapResult = { ms: r.ms }; }); });
  await page.waitForTimeout(250);
  const shotMid = await page.screenshot();
  const midState = await page.evaluate(() => ({ frames: document.querySelectorAll('[data-v4-frame]').length, building: !!document.querySelector('[data-v4-building]'), retiring: !!document.querySelector('[data-v4-retiring]') }));
  await page.evaluate(() => window.__swapDone);
  await page.waitForTimeout(100);
  const probe = await page.evaluate(() => { window.__labMountDelayMs = 0; return window.__probe.stop(); });
  const shotAfter = await page.screenshot();
  fs.writeFileSync(shot('swap-mid-1280.png'), shotMid);
  const sameMid = await pixelSame(shotBefore, shotMid);
  const sameAfter = await pixelSame(shotBefore, shotAfter);
  check('Doppelpuffer: waehrend des Tauschs stehen alter und neuer Rahmen (neu unsichtbar)', midState.frames === 2 && midState.building && midState.retiring, JSON.stringify(midState));
  check('Doppelpuffer: nie weniger als 3 sichtbare Fenster mit Inhalt (MutationObserver + rAF)', probe.min >= 3 && probe.rafMin >= 3 && probe.samples > 0,
    `MO ${probe.samples} Proben min ${probe.min}, rAF ${probe.rafSamples} Proben min ${probe.rafMin}`);
  check('Doppelpuffer: Pixelprobe mitten im Tausch = Bild vorher (>= 99 %)', sameMid >= 0.99, `${(sameMid * 100).toFixed(2)} % gleich, nachher ${(sameAfter * 100).toFixed(2)} %`);
  check('Doppelpuffer: nach dem Tausch wieder genau ein Rahmen', (await page.$$('[data-v4-frame]')).length === 1 && (await page.$$('link[data-v4-frame-css]')).length === 6);

  // --- Listener-Zahl nach 5 Shell-Tauschen konstant ---
  const counts = [];
  for (let i = 0; i < 5; i++) {
    await page.evaluate(() => window.__lab.swapShell());
    await page.waitForTimeout(120);
    counts.push(await page.evaluate((stub) => ({ ...window.__listenerCount(), subs: stub ? null : window.__lab.kernel.sim.subscriberCount(), frames: document.querySelectorAll('[data-v4-frame]').length, wins: document.querySelectorAll('.shell-window').length }), STUB));
  }
  const sig = (c) => `${c.listeners}/${c.observers}/${c.subs}/${c.frames}/${c.wins}`;
  check('5 Shell-Tausche: Listener, Observer, Kernel-Abos, Fenster konstant', counts.every((c) => sig(c) === sig(counts[0])) && counts[0].frames === 1 && counts[0].wins === 3,
    counts.map(sig).join(' | '));
  check('Fensterinhalt nach 6 Tauschen erhalten', (await page.inputValue('.shell-window[data-window-app="notes"] [data-lab-input]')) === 'halb ausgefuellt');
  {
    // Zustandszone im Chrome sichtbar machen (Layout-Nachweis), dann leeren. Im Stub über kernel.status (Strom).
    await page.evaluate(() => {
      const rec = window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'ledger');
      rec.host.instance.api.status.report({ level: 'warn', code: 'demo', text: 'Zwei Änderungen noch nicht übertragen' });
    });
    await page.waitForFunction(() => {
      const z = document.querySelector('.shell-window[data-window-app="ledger"] [data-v4-zone-slot] .v4-status-zone');
      return z && !z.hidden;
    });
    await page.waitForTimeout(80);
    await page.screenshot({ path: shot('zone-1280.png') });
    if (STUB) {
      const viaKernel = await page.evaluate(() => window.__lab.kernel.status.get().some((e) => e.app_id === 'ledger' && e.code === 'demo' && e.source === 'app'));
      check('Zustandseintrag der App laeuft ueber kernel.status (Strom)', viaKernel);
    }
    // FR6a / M-14: Meldungen hängen als Badge am App-Icon; Liste klappt erst auf Klick unter dem Icon auf (im Fenster).
    const LW = '.shell-window[data-window-app="ledger"]';
    const zoneGeo = () => page.$eval(LW, (w) => {
      const z = w.querySelector('[data-v4-zone-slot] .v4-status-zone'); const b = z.querySelector('.v4-status-zone__badge').getBoundingClientRect();
      const i = w.querySelector('.shell-window-v2-icon').getBoundingClientRect(); const wr = w.getBoundingClientRect();
      const list = w.querySelector('.v4-status-zone__list'); const lr = list.getBoundingClientRect();
      return { badge: w.querySelector('.v4-status-zone__badge').textContent, level: z.dataset.level, bg: getComputedStyle(w.querySelector('.v4-status-zone__badge')).backgroundColor,
        atIcon: b.left < i.right + 2 && b.right > i.left + i.width / 2 && b.top < i.bottom + 2 && b.bottom > i.top + i.height / 2,
        listOpen: !list.hidden, listInWin: list.closest('.shell-window') === w && lr.left >= wr.left && lr.right <= wr.right + 0.5,
        listBelowIcon: list.hidden || lr.top >= i.bottom - 0.5, statusbar: !!w.querySelector('.v4-status-zone__line[style*="height"]'),
        rows: [...list.querySelectorAll('.v4-status-zone__item')].map((r) => [...r.querySelectorAll('button')].map((x) => x.textContent).join('+')) };
    });
    // Fenster nach vorn (Taskleiste), damit das Icon nicht verdeckt ist.
    const ledgerWinId = await page.evaluate(() => window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'ledger').id);
    await page.click(`[data-v4-win="${ledgerWinId}"]`);
    await page.waitForTimeout(60);
    const z0 = await zoneGeo();
    await page.click(`${LW} .shell-window-v2-icon`);
    const z1 = await zoneGeo();
    await page.screenshot({ path: shot('fr6a-badge-list-1280.png') });
    await page.keyboard.press('Escape');
    const z2 = await zoneGeo();
    await page.click(`${LW} .v4-status-zone__badge`);
    const z3 = await zoneGeo();
    await page.click(`${LW} .v4-status-zone__badge`);
    check('FR6a Meldungen am Icon: Badge mit Zahl am App-Icon (Warnung gelb), Liste zu bis zum Klick; Klick auf Icon öffnet sie unter dem Icon im Fenster mit „Ausblenden“; Esc schließt; Badge-Klick schaltet um',
      z0.badge === '1' && z0.level === 'warn' && z0.atIcon && !z0.listOpen && z1.listOpen && z1.listInWin && z1.listBelowIcon && JSON.stringify(z1.rows) === '["Ausblenden"]' && !z2.listOpen && z3.listOpen,
      JSON.stringify({ z0, z1: { open: z1.listOpen, inWin: z1.listInWin, below: z1.listBelowIcon, rows: z1.rows }, z2: z2.listOpen, z3: z3.listOpen }));
    // Fehler: Badge rot, Zahl 2, ruhiger Puls, Liste öffnet sich nicht von selbst.
    await page.evaluate(() => window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'ledger').host.instance.api.status.report({ level: 'error', code: 'demo2', text: 'Export fehlgeschlagen', action: { label: 'Erneut', run: () => true } }));
    await page.waitForFunction((w) => document.querySelector(`${w} .v4-status-zone__badge`)?.textContent === '2', LW);
    const z4 = await page.$eval(LW, (w) => ({ level: w.querySelector('.v4-status-zone').dataset.level, pulse: w.querySelector('.v4-status-zone').classList.contains('is-pulse'), open: !w.querySelector('.v4-status-zone__list').hidden, bg: getComputedStyle(w.querySelector('.v4-status-zone__badge')).backgroundColor }));
    await page.click(`${LW} .v4-status-zone__badge`);
    const z5 = await zoneGeo();
    await page.screenshot({ path: shot('fr6a-badge-error-1280.png') });
    await page.click(`${LW} .v4-status-zone__badge`);
    check('FR6a Fehler: Badge rot mit 2, kurzer Puls, öffnet sich nicht selbst; Liste: Fehler oben mit „Erneut“ + „Ausblenden“; keine Statusleiste unten',
      z4.level === 'error' && z4.pulse && !z4.open && z4.bg !== z0.bg && JSON.stringify(z5.rows) === '["Erneut+Ausblenden","Ausblenden"]' && !z5.statusbar,
      JSON.stringify({ z4, rows: z5.rows }));
    await page.evaluate(() => window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'ledger').host.instance.api.status.clear('demo2'));
    await page.evaluate(() => window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'ledger').host.instance.api.status.clear('demo'));
    await page.waitForFunction(() => document.querySelector('.shell-window[data-window-app="ledger"] [data-v4-zone-slot] .v4-status-zone')?.hidden);
  }

  // Neue App per kernel.apps-Push
  await page.evaluate((stub) => (stub ? window.__lab.publish('crm', '2.0.0', 'Kunden') : window.__lab.kernel.sim.addApp(
    { id: 'crm', title: 'Kunden', active_ver: '2.0.0', activated_ver: '2.0.0', state: 'activated', target_ver: null, error: null },
    window.__lab.manifestFor('crm', '2.0.0', 'Vertrieb'),
  )), STUB);
  await page.waitForSelector('[data-v4-app="crm"] img', { state: 'attached' });
  check('Neue App erscheint ohne Reload', (await page.evaluate(() => window.__noReload)) === 'marker' && (await tileIds()).includes('crm'));
  if (STUB) {
    // Das Icon-Bild hängt schon im DOM, bevor src gesetzt ist (Icon-Auflösung asynchron) – auf src warten statt rennen.
    await page.waitForSelector('[data-v4-app="crm"] img[src]', { state: 'attached', timeout: 5000 });
    const crmSrc = await page.$eval('[data-v4-app="crm"] img[src]', (img) => img.getAttribute('src'));
    check('layout.icon_svg wird als Icon gezeigt (data:image/svg+xml)', crmSrc.startsWith('data:image/svg+xml'), crmSrc.slice(0, 40));
  }
  // Entfernte App verschwindet (inkl. offenem Fenster)
  await page.evaluate((stub) => (stub ? window.__lab.remove('tasks') : window.__lab.kernel.sim.removeApp('tasks')), STUB);
  await page.waitForFunction(() => !document.querySelector('[data-v4-app="tasks"]'));
  await page.waitForTimeout(700);
  check('Entfernte App verschwindet aus Launcher', !(await tileIds()).includes('tasks'));
  check('Fenster der entfernten App geschlossen', !(await page.$('.shell-window[data-window-app="tasks"]')) && (await page.$$('[data-v4-win]')).length === 2);

  // Aktivierung einer neuen Version -> Host.swap
  await page.evaluate((stub) => (stub ? window.__lab.publish('ledger', '1.1.0', 'Buchhaltung') : window.__lab.kernel.sim.activate('ledger', '1.1.0', window.__lab.manifestFor('ledger', '1.1.0', 'Finanzen'))), STUB);
  await page.waitForSelector('.shell-window[data-window-app="ledger"] [data-lab-ver="1.1.0"]');
  check('activation.onChange tauscht Fensterinhalt live (Host.swap)', true);
  await page.waitForFunction((sv) => document.querySelector('[data-v4-app="ledger"] img')?.getAttribute('src') === `/~a/ledger@1.1.0~shell@${sv}/icon.svg`, activeShell);
  check('Launcher-Icon folgt neuer Version', true);

  // api.shell.assetUrl(appId, path): versionierte URL der AKTIVEN Version einer beliebigen App aus den Kernel-App-
  // Einträgen, mit gebundener Shell-Version (Form von sw.js URL_RE); unbekannte App bzw. leerer Pfad → null.
  const assetRes = await page.evaluate(() => {
    const rec = window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'notes');
    const { shell } = rec.host.instance.api;
    return {
      shellVer: window.__lab.activeShell(),
      mountOwn: rec.host.assetUrl('./assets/a b.png'),
      ledgerBinds: window.__lab.kernel.sim.binds ? window.__lab.kernel.sim.binds.filter((b) => b === 'ledger@1.1.0').length : null,
      other: shell.assetUrl('ledger', 'icon.svg'),
      own: shell.assetUrl('notes', './assets/a b.png'),
      unknown: shell.assetUrl('gibt-es-nicht', 'icon.svg'),
      emptyPath: shell.assetUrl('ledger', ''),
      oneArg: shell.assetUrl('icon.svg'),
    };
  });
  const swSource = fs.readFileSync(path.join(here, '../../sw.js'), 'utf8');
  const swRe = new RegExp(/const URL_RE = \/(.+)\/;/.exec(swSource)[1]);
  const swMatch = swRe.exec(assetRes.other || '');
  check('api.shell.assetUrl(appId, path): /~a/<app>@<aktive ver>~shell@<shell_ver>/<pfad>, folgt der Aktivierung, unbekannt → null',
    assetRes.other === `/~a/ledger@1.1.0~shell@${assetRes.shellVer}/icon.svg`
      && assetRes.own === `/~a/notes@1.0.0~shell@${assetRes.shellVer}/assets/a%20b.png` // Pfadsegmente kodiert wie kernel.artifacts.url
      && assetRes.unknown === null && assetRes.emptyPath === null && assetRes.oneArg === null
      && !!swMatch && swMatch[1] === 'ledger' && swMatch[2] === '1.1.0' && swMatch[3] === assetRes.shellVer && swMatch[4] === 'icon.svg',
    JSON.stringify(assetRes));
  // K-1d: Code, styles und eigene Dateien eines Fensters laufen über die Bindung seines Mounts (kernel.artifacts.bind),
  // genau ein bind je Mount (Attrappe zählt: Swap ledger 1.0.0 → 1.1.0 = ein bind, Icon-URLs binden nicht).
  check('Bindung je Mount: host.assetUrl über artifacts.bind, genau ein bind je Mount',
    assetRes.mountOwn === `/~a/notes@1.0.0~shell@${assetRes.shellVer}/assets/a%20b.png` && (STUB || assetRes.ledgerBinds === 1),
    JSON.stringify({ mountOwn: assetRes.mountOwn, ledgerBinds: assetRes.ledgerBinds }));

  // Taskleiste: Fensterwechsel
  const winBtn = await page.$('[data-v4-win]');
  const winId = await winBtn.evaluate((b) => b.dataset.v4Win);
  await winBtn.click();
  const focusedNow = await page.$eval(`#${winId}`, (e) => e.classList.contains('is-focused') && e.style.display !== 'none');
  const focusedBefore = focusedNow;
  await page.keyboard.press('Alt+Backquote');
  const cycled = await page.$$eval('.shell-window.is-focused', (els) => els.map((e) => e.id));
  check('Fensterwechsel per Taskleiste und Alt+`', focusedBefore && cycled.length === 1, cycled.join(','));

  // Breiten 900 und 420
  for (const [w, h] of [[900, 760], [420, 860]]) {
    await page.setViewportSize({ width: w, height: h });
    await page.waitForTimeout(400);
    await page.screenshot({ path: shot(`frame-${w}.png`) });
    const overflow = await page.evaluate(() => {
      const bar = document.querySelector('[data-v4-desk]').getBoundingClientRect(); // FR6c: keine Taskleiste unten, Grenze = Desk
      const wins = [...document.querySelectorAll('.shell-window')].filter((e) => e.style.display !== 'none').map((e) => e.getBoundingClientRect());
      return { scrollW: document.documentElement.scrollWidth, vw: innerWidth, barBottom: bar.bottom, vh: innerHeight, winOverBar: wins.some((r) => r.bottom > bar.bottom + 1 || r.top < bar.top - 1), winOutside: wins.some((r) => r.left < -1 || r.right > innerWidth + 1) };
    });
    check(`Breite ${w}: kein horizontaler Ueberlauf, Fenster nicht ueber Taskleiste/Rand`, overflow.scrollW <= overflow.vw && !overflow.winOverBar && !overflow.winOutside, JSON.stringify(overflow));
  }
  // Launcher-Ansicht schmal (Desktop zeigen)
  await startOpen(page); // FR6d: Startknopf öffnet das Startmenü, sein Fuß „Desktop“ legt den Desktop frei
  await page.click('[data-v4-launcher-desktop]');
  await page.waitForTimeout(700);
  await page.screenshot({ path: shot('frame-420-launcher.png') });
  check('Schmal: Desktop-Knopf zeigt Launcher', (await page.$$eval('.shell-window', (els) => els.filter((e) => e.style.display !== 'none' && e.getBoundingClientRect().width > 0).length)) === 0);
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.waitForTimeout(300);
  await page.screenshot({ path: shot('frame-1280-desktop.png') });

  // ================= FR2: Aufgaben der App-Ports an den Rahmen (frische Seite je Modus) =================
  {
    const F = await browser.newPage({ viewport: { width: 1280, height: 800 } });
    F.on('pageerror', (e) => errors.push(`[FR2] ${String(e)}`));
    F.on('console', (m) => { if (m.type() === 'error') errors.push(`[FR2] ${m.text()}`); });
    await F.goto(`${base}/v4/frame/lab/${STUB ? 'stub-lab' : 'index'}.html${STUB ? `?inst=fr2-${Date.now().toString(36)}` : ''}`);
    await F.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
    await F.waitForFunction(() => document.querySelectorAll('[data-v4-launcher] [data-v4-app] img').length === 3);
    await F.evaluate((stub) => (stub ? window.__lab.publish('crm', '2.0.0', 'Kunden') : window.__lab.kernel.sim.addApp(
      { id: 'crm', title: 'Kunden', active_ver: '2.0.0', activated_ver: '2.0.0', state: 'activated', target_ver: null, error: null },
      window.__lab.manifestFor('crm', '2.0.0', 'Vertrieb'),
    )), STUB);
    await F.waitForSelector('[data-v4-app="crm"] img', { state: 'attached' });
    const openCrm = (g) => F.evaluate(async ([g]) => {
      const h = window.__lab.frame.shellHandle();
      const owner = h.openWindow('crm', { geometry: g });
      await h.win(owner).host.ready;
      return owner;
    }, [g]);

    // ---- FR2-1: Container business-app-window je Fenster, Haltepunkte 1024/768 ohne eigene Deklaration ----
    const owners = [];
    for (const w of [1100, 900, 600]) owners.push(await openCrm({ x: 10, y: 10, w, h: 420 }));
    await F.waitForTimeout(400);
    const bp = await F.evaluate((owners) => owners.map((o) => {
      const el = window.__lab.frame.shellHandle().win(o).handle.element;
      const c = el.querySelector('.shell-window-content');
      const cs = getComputedStyle(c);
      return { w: Math.round(c.getBoundingClientRect().width), bp: getComputedStyle(el.querySelector('[data-crm-bp]')).getPropertyValue('--crm-bp').trim(), name: cs.containerName, type: cs.containerType };
    }), owners);
    check('FR2-1 Inhaltsbereich ist Size-Container business-app-window, Haltepunkte 1024/768 greifen ohne App-Deklaration',
      bp.every((b) => b.name === 'business-app-window' && b.type === 'inline-size') && bp.map((b) => b.bp).join(',') === 'wide,mid,narrow', JSON.stringify(bp));
    // Doppelte Deklaration (App-root deklariert den Container selbst, wie placements/intake): gleiche Haltepunkte.
    const dbl = await F.evaluate((owners) => owners.map((o) => {
      const el = window.__lab.frame.shellHandle().win(o).handle.element;
      el.querySelector('.v4-app-root').style.container = 'business-app-window / inline-size';
      return getComputedStyle(el.querySelector('[data-crm-bp]')).getPropertyValue('--crm-bp').trim();
    }), owners);
    check('FR2-1 doppelte Container-Deklaration (App-root + Rahmen) unschädlich', dbl.join(',') === 'wide,mid,narrow', dbl.join(','));
    await F.evaluate((owners) => owners.forEach((o) => { window.__lab.frame.shellHandle().win(o).handle.element.querySelector('.v4-app-root').style.container = ''; }), owners);

    // ---- FR2-2: v2-Reservezonen im Inhaltsbereich auf 0 ----
    const zones = await F.evaluate((o) => {
      const el = window.__lab.frame.shellHandle().win(o).handle.element;
      const head = getComputedStyle(el.querySelector('[data-crm-head]'));
      const r18 = getComputedStyle(el.querySelector('[data-crm-r18]'));
      const content = getComputedStyle(el.querySelector('.shell-window-content'));
      const cl = el.querySelector('[data-window-controls]').getBoundingClientRect();
      return { padL: head.paddingLeft, padR: head.paddingRight, h: head.height, r18: r18.paddingRight, icon: content.getPropertyValue('--shell-v2-icon-size').trim(), chromeIcon: getComputedStyle(el).getPropertyValue('--shell-v2-icon-size').trim(),
        ctrlInset: content.getPropertyValue('--shell-controls-inset').trim(), cluster: Math.ceil(cl.width), iconBox: Math.round(el.querySelector('.shell-window-v2-icon').getBoundingClientRect().width) };
    }, owners[0]);
    // FR6a (Owner 15.10.): Icon und Knopfcluster schweben wieder über dem Inhalt. Die v2-Reservezonen zeigen auf die
    // reservierten Zonen des Rahmens: Kopf links = Icon (36) + 14, rechte Steuerzone = gemessene Clusterbreite + 8.
    check('FR6a v2-Reservezonen wie Shell-V2: --shell-v2-icon-size 80 px (Kopf links 94 px, Icon-Header 80 px), --shell-v2-control-inset = --shell-controls-inset = Cluster + 8',
      zones.padL === '94px' && zones.icon === '80px' && zones.iconBox === 80 && zones.r18 === zones.ctrlInset && parseFloat(zones.r18) === zones.cluster + 8, JSON.stringify(zones));
    await F.evaluate((owners) => { const h = window.__lab.frame.shellHandle(); for (const o of owners.slice(1)) if (h.win(o)) h.closeWindow(o); }, owners);
    await F.waitForTimeout(300);

    // ---- Zusatz: data-app nur am Inhalts-root; Theme-Schnittstelle (Tokens + base.css) aus dem Shell-Artefakt ----
    const iface = await F.evaluate(() => {
      const root = document.querySelector('.v4-app-root[data-app="crm"]');
      const cs = getComputedStyle(root);
      return {
        chromeWithDataApp: document.querySelectorAll('.shell-window[data-app], .shell-window [data-app]:not(.v4-app-root):not(.v4-app-root *)').length,
        rootsWithDataApp: document.querySelectorAll('.v4-app-root[data-app]').length,
        windows: document.querySelectorAll('.shell-window').length,
        windowApp: document.querySelector('.shell-window[data-window-app="crm"]') ? 'crm' : '',
        links: [...document.querySelectorAll('link[data-v4-frame-css]')].map((l) => new URL(l.href).pathname),
        surface: cs.getPropertyValue('--surface').trim(), space: cs.getPropertyValue('--space-2').trim(), text: cs.getPropertyValue('--text-strong').trim(),
      };
    });
    check('data-app nur am Inhalts-root (Chrome: data-window-app), App-CSS [data-app] trifft keine Fensterknöpfe',
      iface.chromeWithDataApp === 0 && iface.rootsWithDataApp === iface.windows && iface.windowApp === 'crm', JSON.stringify(iface));
    check('Theme-Schnittstelle: theme.css + shared/base.css + shared/shell-v2-chrome.css + frame.css + shared/shell-v2-topbar.css + shell-v2-desktop.css (FR6d) aus dem Shell-Artefakt, Tokens im App-root gesetzt',
      iface.links.join(',') === '/v4/frame/theme/theme.css,/shared/base.css,/shared/shell-v2-chrome.css,/v4/frame/frame.css,/shared/shell-v2-topbar.css,/v4/frame/shell-v2-desktop.css' && iface.surface && iface.space === '8px' && iface.text, JSON.stringify({ links: iface.links, surface: iface.surface, space: iface.space }));

    // ---- FR2-3: Spaltenteiler verdrahtet, Breite je Fenster in window_state, nach Remount (Shell-Tausch) wieder da ----
    const crm = owners[0];
    const handleSel = `#${await F.evaluate((o) => window.__lab.frame.shellHandle().win(o).handle.element.id, crm)} [data-resizer-var="--crm-left"]`;
    const hb = await (await F.$(handleSel)).boundingBox();
    await F.mouse.move(hb.x + hb.width / 2, hb.y + 40);
    await F.mouse.down();
    await F.mouse.move(hb.x + hb.width / 2 + 40, hb.y + 40, { steps: 4 });
    await F.mouse.move(hb.x + hb.width / 2 + 80, hb.y + 40, { steps: 4 });
    await F.mouse.up();
    await F.focus(handleSel);
    await F.keyboard.press('ArrowRight');
    await F.waitForTimeout(250);
    const rz = await F.evaluate(async (o) => {
      const h = window.__lab.frame.shellHandle();
      const rec = h.win(o);
      const split = rec.handle.element.querySelector('[data-crm-split]');
      const left = rec.handle.element.querySelector('[data-crm-left]');
      return {
        role: rec.handle.element.querySelector('[data-resizer-var]').getAttribute('role'),
        cssVar: split.style.getPropertyValue('--crm-left'),
        leftW: Math.round(left.getBoundingClientRect().width),
        stored: await window.__lab.kernel.windowState.get(`cols:crm:${o}`),
        appDefault: await window.__lab.kernel.windowState.get('cols:crm'),
      };
    }, crm);
    check('FR2-3 Spaltenteiler im Inhaltsbereich verdrahtet (Zeiger + Tastatur), Breite je Fenster in window_state',
      rz.role === 'separator' && rz.cssVar === '324px' && rz.leftW === 324 && rz.stored?.widths?.['--crm-left'] === 324 && rz.stored?.app === 'crm' && rz.appDefault?.widths?.['--crm-left'] === 324, JSON.stringify(rz));
    await F.evaluate(() => window.__lab.swapShell());
    await F.waitForSelector('[data-v4-ready] .shell-window[data-window-app="crm"] [data-crm-split]');
    await F.waitForTimeout(150);
    const afterSwap = await F.evaluate((o) => {
      const rec = window.__lab.frame.shellHandle().win(o);
      const split = rec?.handle.element.querySelector('[data-crm-split]');
      return { cssVar: split?.style.getPropertyValue('--crm-left') || '', role: rec?.handle.element.querySelector('[data-resizer-var]')?.getAttribute('role') || '' };
    }, crm);
    await F.focus(handleSel.replace(/^#[^ ]+/, `#${await F.evaluate((o) => window.__lab.frame.shellHandle().win(o).handle.element.id, crm)}`));
    await F.keyboard.press('ArrowRight');
    await F.waitForTimeout(250);
    const afterKey = await F.evaluate((o) => window.__lab.frame.shellHandle().win(o).handle.element.querySelector('[data-crm-split]').style.getPropertyValue('--crm-left'), crm);
    check('FR2-3 nach Remount (Shell-Tausch): Breite wiederhergestellt und Griff neu verdrahtet', afterSwap.cssVar === '324px' && afterSwap.role === 'separator' && afterKey === '348px', JSON.stringify({ afterSwap, afterKey }));
    const fresh = await openCrm({ x: 200, y: 60, w: 900, h: 420 });
    const freshW = await F.evaluate((o) => window.__lab.frame.shellHandle().win(o).handle.element.querySelector('[data-crm-split]').style.getPropertyValue('--crm-left'), fresh);
    check('FR2-3 neues Fenster derselben App startet mit der zuletzt gezogenen Breite (cols:<app>)', freshW === '348px', freshW);
    await F.evaluate((o) => window.__lab.frame.shellHandle().closeWindow(o), fresh);
    await F.waitForTimeout(300);
    const forgotten = await F.evaluate(async (o) => window.__lab.kernel.windowState.get(`cols:crm:${o}`), fresh);
    check('FR2-3 Schließen löscht den Fensterschlüssel', forgotten == null, JSON.stringify(forgotten));

    // ---- FR2-7: navigate(app, params) reicht RouteEvent {from, to, params} durch; Chat-Wunsch und Crew-Auslastung ----
    const nav = await F.evaluate(async () => {
      const h = window.__lab.frame.shellHandle();
      const owner = h.openWindow('notes');
      await h.win(owner).host.ready;
      const api = h.win(owner).host.instance.api;
      const crmCount = () => h.windows().filter((r) => r.appId === 'crm').length;
      const before = crmCount();
      const ok = api.shell.navigate('crm', { chat: { commandId: 'cmd-1', taskId: 't-1' } });
      await new Promise((r) => setTimeout(r, 50));
      const top = h.windows().filter((r) => r.appId === 'crm').sort((a, b) => Number(b.handle.element.style.zIndex) - Number(a.handle.element.style.zIndex))[0];
      const ev = JSON.parse(top.host.instance.root.dataset.crmRoute || 'null');
      const unknown = api.shell.navigate('ctox', { chat: { taskId: 'x' } });
      api.shell.navigate('notes'); // zurück
      return { ok, ev, sameWindow: crmCount() === before, unknown, notesOwner: owner };
    });
    check('FR2-7 navigate reicht {chat} unverändert als RouteEvent {from, to, params} durch (offenes Fenster)',
      nav.ok && nav.sameWindow && nav.ev?.from === 'notes' && nav.ev?.to === 'crm' && nav.ev?.params?.chat?.commandId === 'cmd-1' && nav.ev?.params?.chat?.taskId === 't-1', JSON.stringify(nav.ev));
    check('FR2-7 navigate auf eine fehlende App liefert false (keine App-Sonderfälle im Rahmen)', nav.unknown === false, String(nav.unknown));
    const navNew = await F.evaluate(async () => {
      const h = window.__lab.frame.shellHandle();
      for (const r of h.windows().filter((r) => r.appId === 'crm')) h.closeWindow(r.ownerId);
      await new Promise((r) => setTimeout(r, 300));
      const notes = h.windows().find((r) => r.appId === 'notes');
      notes.host.instance.api.shell.navigate('crm', { chat: { taskId: 'neu' } });
      const rec = h.windows().find((r) => r.appId === 'crm');
      await rec.host.ready;
      await new Promise((r) => setTimeout(r, 20));
      return JSON.parse(rec.host.instance.root.dataset.crmRoute || 'null');
    });
    check('FR2-7 navigate öffnet fehlendes Fenster und liefert die Parameter nach mount', navNew?.from === 'notes' && navNew?.params?.chat?.taskId === 'neu', JSON.stringify(navNew));
    // Geschlossene App: Start-Route als ctx.route an mount UND genau einmal als erstes onRoute-Ereignis – auch wenn
    // die App onRoute erst nach mount abonniert (browser OFFEN-9: Startargumente gingen verloren).
    const navClosed = await F.evaluate(async () => {
      const h = window.__lab.frame.shellHandle();
      const out = {};
      for (const late of [false, true]) {
        for (const r of h.windows().filter((r) => r.appId === 'crm')) h.closeWindow(r.ownerId);
        await new Promise((r) => setTimeout(r, 300));
        globalThis.__labLateRoute = late;
        const notes = h.windows().find((r) => r.appId === 'notes');
        const params = { login: { url: 'https://example.test/anmelden', user: 'lab' }, n: late ? 2 : 1 };
        const ok = notes.host.instance.api.shell.navigate('crm', params);
        const rec = h.windows().find((r) => r.appId === 'crm');
        await rec.host.ready;
        await new Promise((r) => setTimeout(r, 200));
        const root = rec.host.instance.root;
        out[late ? 'late' : 'sync'] = { ok, mount: JSON.parse(root.dataset.crmMountRoute), route: JSON.parse(root.dataset.crmRoute || 'null'), count: Number(root.dataset.crmRouteCount || 0), expect: params };
      }
      globalThis.__labLateRoute = false;
      return out;
    });
    const exact = (r) => r.ok && JSON.stringify(r.mount?.params) === JSON.stringify(r.expect) && r.mount?.from === 'notes' && r.mount?.to === 'crm'
      && JSON.stringify(r.route) === JSON.stringify(r.mount) && r.count === 1;
    check('navigate auf geschlossene App: Fenster geht auf, ctx.route und erstes onRoute tragen exakt die params (auch bei spätem onRoute)',
      exact(navClosed.sync) && exact(navClosed.late), JSON.stringify(navClosed));

    // ---- Zone: Aktion am Eintrag (item.action {label, run}) ----
    const zoneOwner = nav.notesOwner;
    await F.evaluate(() => { const h = window.__lab.frame.shellHandle(); for (const r of h.windows().filter((r) => r.appId === 'crm')) h.closeWindow(r.ownerId); });
    await F.waitForTimeout(400);
    await F.evaluate((o) => {
      const api = window.__lab.frame.shellHandle().win(o).host.instance.api;
      window.__ran = 0;
      window.__lab.frame.shellHandle().win(o).host.zone.clear('shell.navigate'); // Meldung aus dem navigate('ctox')-Fehlversuch oben
      api.status.report({ level: 'warn', code: 'fail', text: 'Abgleich offen', action: { label: 'Erneut', run: () => { throw new Error('nein'); } } });
      api.status.report({ level: 'warn', code: 'stale', text: 'Basis ist nicht mehr aktiv', action: { label: 'Auf aktuelle Version setzen', run: async () => { window.__ran++; } } });
    }, zoneOwner);
    const zsel = `#${await F.evaluate((o) => window.__lab.frame.shellHandle().win(o).handle.element.id, zoneOwner)}`;
    // FR6a: Aktionen stehen in der Liste am Icon (Badge-Klick öffnet sie), je Meldung eigene Aktion + „Ausblenden“.
    const ownAct = (s) => document.querySelector(`${s} .v4-status-zone__list:not([hidden]) .v4-status-zone__item .v4-status-zone__action:not([data-hide])`)?.textContent;
    await F.waitForFunction((s) => document.querySelector(`${s} .v4-status-zone__badge`)?.textContent === '2', zsel);
    await F.click(`${zsel} .v4-status-zone__badge`);
    await F.waitForFunction((s) => document.querySelector(`${s} .v4-status-zone__list:not([hidden]) .v4-status-zone__item .v4-status-zone__action:not([data-hide])`)?.textContent === 'Auf aktuelle Version setzen', zsel);
    await F.waitForTimeout(80);
    await F.screenshot({ path: shot('zone-action-1280.png') });
    await F.click(`${zsel} .v4-status-zone__list .v4-status-zone__item .v4-status-zone__action:not([data-hide])`);
    await F.waitForFunction((s) => document.querySelector(`${s} [data-v4-zone-slot] .v4-status-zone__line`)?.textContent === 'Abgleich offen', zsel);
    const z1 = await F.evaluate((s) => ({ ran: window.__ran, btn: document.querySelector(`${s} .v4-status-zone__list:not([hidden]) .v4-status-zone__item .v4-status-zone__action:not([data-hide])`)?.textContent || '' }), zsel);
    await F.click(`${zsel} .v4-status-zone__list .v4-status-zone__item .v4-status-zone__action:not([data-hide])`);
    await F.waitForTimeout(150);
    void ownAct;
    const z2 = await F.evaluate((s) => {
      const w = document.querySelector(s);
      return { zoneVisible: !w.querySelector('.v4-status-zone').hidden, line: w.querySelector('.v4-status-zone__line').textContent, listButtons: [...w.querySelectorAll('.v4-status-zone__list:not([hidden]) .v4-status-zone__item .v4-status-zone__action')].map((b) => b.textContent) };
    }, zsel);
    check('Zone: Aktion als Knopf am Eintrag; Klick ruft run() und schließt den Eintrag; Fehlschlag lässt ihn stehen; Knopf auch in der Liste',
      z1.ran === 1 && z1.btn === 'Erneut' && z2.zoneVisible && z2.line === 'Abgleich offen' && z2.listButtons.join(',') === 'Erneut,Ausblenden', JSON.stringify({ z1, z2 }));
    await F.evaluate((o) => window.__lab.frame.shellHandle().win(o).host.instance.api.status.clear('fail'), zoneOwner);
    await F.waitForFunction((s) => document.querySelector(`${s} .v4-status-zone`)?.hidden, zsel);

    // ================= FR4: Spaltengrammatik, Zonen-Lebensdauer, Quittieren, navigate-Ziele, gesperrte Knöpfe =================
    // ---- FR4-1: Rahmen verdrahtet data-pg-* nach mount/Remount und für später gerenderte Panes; selbst verdrahtete bleiben ----
    {
      await F.evaluate(() => { globalThis.__labGrammar = true; });
      const pgOwner = await openCrm({ x: 40, y: 40, w: 900, h: 420 });
      await F.waitForTimeout(120);
      const probePg = (o) => F.evaluate((o) => {
        const root = window.__lab.frame.shellHandle().win(o).host.instance.root;
        const pane = root.querySelector('[data-crm-pg]');
        if (!pane) return { pane: false };
        let events = 0;
        pane.addEventListener('ctox-pane-grammar-change', () => { events++; });
        const search = pane.querySelector('[data-pg-search]');
        search.value = 'kunde'; search.dispatchEvent(new Event('input', { bubbles: true }));
        pane.querySelector('[data-pg-tray-toggle]').click();
        const trayOpen = !pane.querySelector('[data-pg-tray]').hidden;
        pane.querySelector('[data-pg-band="offen"]').click();
        return { pane: true, wired: pane.dataset.pgWired, handle: typeof pane.__ctoxPaneGrammar?.setCounts === 'function', events, trayOpen,
          band: pane.__ctoxPaneGrammar?.state?.().band, dot: pane.querySelector('[data-pg-tray-toggle]').classList.contains('has-active-filters') };
      }, o);
      const pgMount = await probePg(pgOwner);
      check('FR4-1 Spaltengrammatik nach mount vom Rahmen verdrahtet (Suche meldet, Ablage öffnet, Band wählt, Punkt aktiv), App ohne eigenen Aufruf',
        pgMount.pane && pgMount.wired === 'true' && pgMount.handle && pgMount.events === 2 && pgMount.trayOpen && pgMount.band === 'offen' && pgMount.dot, JSON.stringify(pgMount));
      // Später gerenderte Pane (Datenabo/Ansichtswechsel) und eine von der App selbst verdrahtete Pane (eigene Modulinstanz).
      const pgLate = await F.evaluate(async (o) => {
        const root = window.__lab.frame.shellHandle().win(o).host.instance.root;
        const mk = (id) => { const s = document.createElement('section'); s.className = 'ctox-pane'; s.dataset.late = id; s.innerHTML = '<input data-pg-search><button type="button" data-pg-tray-toggle>F</button><div data-pg-tray hidden></div>'; return s; };
        const late = mk('late');
        root.querySelector('[data-crm-left]').append(late);
        const own = await import(`${location.origin}/shared/pane-grammar.js?inst=app-${Date.now()}`);
        const self = mk('self');
        const wrap = document.createElement('div');
        wrap.append(self);
        root.querySelector('[data-crm-left]').append(wrap);
        own.autoWirePaneGrammar(wrap); // nur die eigene Pane, wie eine App mit eigenem Aufruf
        const selfHandle = self.__ctoxPaneGrammar;
        await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
        let selfEvents = 0;
        self.addEventListener('ctox-pane-grammar-change', () => { selfEvents++; });
        const s = self.querySelector('[data-pg-search]'); s.value = 'x'; s.dispatchEvent(new Event('input', { bubbles: true }));
        self.querySelector('[data-pg-tray-toggle]').click();
        return { lateWired: late.dataset.pgWired === 'true' && !!late.__ctoxPaneGrammar, selfSame: self.__ctoxPaneGrammar === selfHandle && !!selfHandle, selfEvents, selfTrayOpen: !self.querySelector('[data-pg-tray]').hidden };
      }, pgOwner);
      check('FR4-1 später gerenderte Pane verdrahtet; selbst verdrahtete Pane bleibt einfach verdrahtet (gleicher Griff, 1 Ereignis, Ablage schaltet 1×)',
        pgLate.lateWired && pgLate.selfSame && pgLate.selfEvents === 1 && pgLate.selfTrayOpen, JSON.stringify(pgLate));
      await F.evaluate(() => window.__lab.swapShell());
      await F.waitForSelector('[data-v4-ready] .shell-window[data-window-app="crm"] [data-crm-pg]');
      await F.waitForTimeout(150);
      const pgSwap = await probePg(pgOwner);
      check('FR4-1 nach Remount (Shell-Tausch) neu verdrahtet', pgSwap.pane && pgSwap.wired === 'true' && pgSwap.events === 2 && pgSwap.trayOpen, JSON.stringify(pgSwap));
      await F.evaluate((o) => { globalThis.__labGrammar = false; window.__lab.frame.shellHandle().closeWindow(o); }, pgOwner);
      await F.waitForTimeout(300);
    }

    // ---- FR4-2: info verschwindet nach 6 s (ohne Wiederaufleben), success nie, warn/error mit „Ausblenden“ je Tab ----
    {
      // Fenster-id nach dem Shell-Tausch in FR4-1 neu lesen (das Chrome-Element ist neu).
      const zid = async () => `#${await F.evaluate((o) => window.__lab.frame.shellHandle().win(o).handle.element.id, zoneOwner)}`;
      let zsel = await zid();
      await F.evaluate((o) => {
        const rec = window.__lab.frame.shellHandle().win(o);
        rec.host.instance.api.status.report({ level: 'info', code: 'export', text: 'Export erstellt.' });
        // 'success' lehnt der Kern-Stub an api.status ab (invalid); direkt an die Zone gemeldet bleibt es unsichtbar.
        rec.host.zone.report({ level: 'success', code: 'ok', text: 'Gespeichert.', source: 'shell' });
        rec.host.zone.report({ level: 'info', code: 'shell.hinweis', text: 'Hinweis', source: 'shell' });
      }, zoneOwner);
      await F.waitForFunction((s) => document.querySelector(`${s} .v4-status-zone__line`)?.textContent === 'Export erstellt.' || document.querySelector(`${s} .v4-status-zone__line`)?.textContent === 'Hinweis', zsel);
      const zi0 = await F.evaluate((o) => window.__lab.frame.shellHandle().win(o).host.zone.entries().map((e) => `${e.level}:${e.code}`).sort(), zoneOwner);
      await F.waitForTimeout(6400);
      const zi1 = await F.evaluate(async (o) => {
        const rec = window.__lab.frame.shellHandle().win(o);
        const vis = !document.querySelector(`#${rec.handle.element.id} .v4-status-zone`).hidden;
        const n = rec.host.zone.entries().length;
        rec.host.zone.report({ level: 'info', code: 'shell.hinweis', text: 'Hinweis', source: 'shell' }); // Strom liefert erneut
        const again = rec.host.zone.entries().length;
        rec.host.zone.report({ level: 'info', code: 'shell.hinweis', text: 'Hinweis 2', source: 'shell' }); // neuer Inhalt
        const fresh = rec.host.zone.entries().map((e) => e.text);
        rec.host.zone.clear('shell.hinweis');
        const kernelRow = window.__lab.kernel.status?.get ? window.__lab.kernel.status.get().some((e) => e.code === 'export') : null;
        return { vis, n, again, fresh, kernelRow };
      }, zoneOwner);
      check('FR4-2 info nach 6 s weg, gleicher Inhalt lebt nicht wieder auf, neuer Inhalt erscheint; success nie angezeigt; Kernel-Eintrag geräumt',
        zi0.join(',') === 'info:app:export,info:shell.hinweis' && !zi1.vis && zi1.n === 0 && zi1.again === 0 && zi1.fresh.join(',') === 'Hinweis 2' && zi1.kernelRow !== true, JSON.stringify({ zi0, zi1 }));
      // warn/error ohne eigene Aktion: „Ausblenden“; quittiert je Tab (sessionStorage), überlebt Strom-Wiederholung und Shell-Tausch.
      const ERR = { level: 'error', code: 'command:fr4-1', text: 'Befehl „demo.run“ fehlgeschlagen: kaputt', ref: 'fr4-1', source: 'shell' };
      await F.evaluate(([o, e]) => window.__lab.frame.shellHandle().win(o).host.zone.report(e), [zoneOwner, ERR]);
      await F.waitForFunction((s) => !document.querySelector(`${s} .v4-status-zone`)?.hidden, zsel);
      await F.click(`${zsel} .v4-status-zone__badge`);
      await F.waitForFunction((s) => document.querySelector(`${s} .v4-status-zone__list:not([hidden]) .v4-status-zone__action[data-hide]`)?.textContent === 'Ausblenden', zsel);
      await F.click(`${zsel} .v4-status-zone__list .v4-status-zone__action[data-hide]`);
      await F.waitForFunction((s) => document.querySelector(`${s} .v4-status-zone`)?.hidden, zsel);
      const za = await F.evaluate(([o, e]) => {
        const z = window.__lab.frame.shellHandle().win(o).host.zone;
        z.report(e);
        return { afterRepeat: z.entries().length, stored: Object.keys(sessionStorage).filter((k) => k.startsWith('ctox-v4-zone-ack:')).length };
      }, [zoneOwner, ERR]);
      await F.evaluate(() => window.__lab.swapShell());
      await F.waitForSelector('[data-v4-ready]');
      await F.waitForTimeout(150);
      zsel = await zid();
      const zb = await F.evaluate(([o, e]) => {
        const z = window.__lab.frame.shellHandle().win(o).host.zone;
        z.report(e);
        const afterSwap = z.entries().length;
        z.clear(e.code);
        z.report(e); // Wiederauftreten nach Räumen: wieder sichtbar
        const reappear = z.entries().map((x) => `${x.code}|${x.action?.label}`);
        z.clear(e.code);
        return { afterSwap, reappear };
      }, [zoneOwner, ERR]);
      check('FR4-2 error ohne eigene Aktion: Knopf „Ausblenden“; quittiert je Tab (Strom-Wiederholung und Shell-Tausch bleiben still); nach clear wieder sichtbar',
        za.afterRepeat === 0 && za.stored >= 1 && zb.afterSwap === 0 && zb.reappear.join(',') === 'command:fr4-1|Ausblenden', JSON.stringify({ za, zb }));
    }

    // ---- FR4-3: navigate auf Ziele ohne v4-App (code-editor, shell/settings): false + genau ein Zoneneintrag ----
    {
      const nv = await F.evaluate((o) => {
        const rec = window.__lab.frame.shellHandle().win(o);
        const api = rec.host.instance.api;
        const a = api.shell.navigate('code-editor', { moduleId: 'x' });
        const b = api.shell.navigate('code-editor', { moduleId: 'x' });
        const one = rec.host.zone.entries().filter((e) => e.code === 'shell.navigate').map((e) => `${e.level}|${e.text}|${e.action?.label}`);
        const c = api.shell.navigate('shell', { open: 'settings', tab: 'channels' });
        const two = rec.host.zone.entries().filter((e) => e.code === 'shell.navigate').map((e) => e.text);
        rec.host.zone.clear('shell.navigate');
        return { a, b, c, one, two };
      }, zoneOwner);
      check('FR4-3 navigate("code-editor") und navigate("shell", {open:"settings"}): false, ein Zoneneintrag der aufrufenden App (mit „Ausblenden“)',
        nv.a === false && nv.b === false && nv.c === false && nv.one.length === 1 && nv.one[0] === 'error|Ziel „code-editor“ ist nicht vorhanden|Ausblenden'
        && nv.two.length === 1 && nv.two[0] === 'Ziel „shell“ ist nicht vorhanden', JSON.stringify(nv));
    }

    // ---- FR4-5: api.shell.openSource(appId?) – Quelltext-Ansicht als Rahmen-Fenster (nur apps.source.view + kernel.source) ----
    {
      const srcSel = '.shell-window[data-window-view="source"][data-window-source="notes"]';
      const zoneOf = (o) => F.evaluate((o) => window.__lab.frame.shellHandle().win(o).host.zone.entries()
        .filter((e) => e.code === 'shell.openSource').map((e) => `${e.level}|${e.text}|${e.action?.label}`), o);
      if (!STUB) {
        // Attrappe ohne kernel.source: kein Einstieg → false + genau ein Zoneneintrag, kein Fenster.
        const r = await F.evaluate((o) => {
          const api = window.__lab.frame.shellHandle().win(o).host.instance.api;
          return { a: api.shell.openSource(), b: api.shell.openSource('notes'), win: !!document.querySelector('.shell-window[data-window-view="source"]') };
        }, zoneOwner);
        const z = await zoneOf(zoneOwner);
        await F.evaluate((o) => window.__lab.frame.shellHandle().win(o).host.zone.clear('shell.openSource'), zoneOwner);
        check('FR4-5 openSource ohne kernel.source: false, kein Fenster, ein Zoneneintrag „Quelltext von „notes“ ist nicht verfügbar“ (mit „Ausblenden“)',
          r.a === false && r.b === false && !r.win && z.length === 1 && z[0] === 'error|Quelltext von „notes“ ist nicht verfügbar|Ausblenden', JSON.stringify({ r, z }));
      } else {
        // FR6a: kein eigenes Quelltext-Fenster mehr – openSource schaltet das App-Fenster selbst auf „Code“.
        // App-Menü am Icon-Header: Fenster nach vorn (falls nötig), Icon klicken, Menüpunkt wählen.
        const pickMenu = async (sel, id) => {
          const winId = await F.$eval(sel, (w) => w.id);
          const focused = await F.$eval(sel, (w) => w.classList.contains('is-focused'));
          if (!focused) { await F.click(`[data-v4-win="${winId}"]`); await F.waitForTimeout(80); }
          await F.click(`${sel} .shell-window-v2-icon`, { position: { x: 30, y: 30 } });
          await F.click(`${sel} [data-v4-appmenu="${id}"]`);
        };
        const notesSel = `#${await F.evaluate((o) => window.__lab.frame.shellHandle().win(o).handle.element.id, zoneOwner)}`;
        const codeSel = `${notesSel}[data-v4-mode="code"] > [data-v4-src-host]:not([hidden]) [data-v4-source="notes"]`;
        const perms0 = await F.evaluate((o) => window.__lab.frame.shellHandle().win(o).host.instance.api.actor.permissions.get(), zoneOwner);
        const winsBefore = await F.$$eval('.shell-window', (els) => els.length);
        const r1 = await F.evaluate((o) => window.__lab.frame.shellHandle().win(o).host.instance.api.shell.openSource(), zoneOwner);
        const opened = await F.waitForSelector(codeSel, { timeout: 6000 }).then(() => true, () => false);
        const r2 = await F.evaluate((o) => {
          const h = window.__lab.frame.shellHandle();
          const again = h.win(o).host.instance.api.shell.openSource('notes');
          const unknown = h.win(o).host.instance.api.shell.openSource('gibt-es-nicht');
          return { again, unknown, sourceWindows: document.querySelectorAll('.shell-window[data-window-view="source"]').length,
            notesWindows: document.querySelectorAll('.shell-window[data-window-app="notes"]').length, windows: document.querySelectorAll('.shell-window').length,
            appKept: !!h.win(o).host.instance.root.querySelector('[data-lab-input]') };
        }, zoneOwner);
        const z1 = await zoneOf(zoneOwner);
        await F.evaluate((o) => window.__lab.frame.shellHandle().win(o).host.zone.clear('shell.openSource'), zoneOwner);
        check('FR6a openSource() schaltet das App-Fenster selbst auf „Code“ (kein eigenes Quelltext-Fenster), App bleibt montiert; zweiter Aufruf bleibt im selben Fenster; unbekannte App → false + ein Zoneneintrag',
          r1 === true && opened && r2.again === true && r2.sourceWindows === 0 && r2.notesWindows === 1 && r2.windows === winsBefore && r2.appKept && r2.unknown === false
            && z1.length === 1 && z1[0] === 'error|Quelltext von „gibt-es-nicht“ ist nicht verfügbar|Ausblenden', JSON.stringify({ r1, opened, r2, z1 }));
        // App ↔ Code mit erhaltenem Entwurf und Cursor: tippen, auf „App“, zurück auf „Code“, Esc, Strg+Umschalt+E.
        const ta = `${notesSel} [data-v4-src-host] textarea`;
        const hasTa = await F.waitForSelector(ta, { timeout: 6000 }).then(() => true, () => false);
        if (hasTa) {
          await F.click(ta);
          await F.keyboard.press('End');
          await F.keyboard.type(' /* fr6a-entwurf */');
          const before = await F.$eval(ta, (t) => { t.dataset.fr6aMark = '1'; return { v: t.value, pos: t.selectionStart }; });
          await F.screenshot({ path: shot('fr6a-code-1280.png') });
          await pickMenu(notesSel, 'app');
          const inApp = await F.$eval(notesSel, (w) => ({ mode: w.dataset.v4Mode, appVisible: getComputedStyle(w.querySelector('.v4-app-root')).visibility !== 'hidden', codeHidden: w.querySelector('[data-v4-src-host]').hidden }));
          await pickMenu(notesSel, 'code');
          const back = await F.$eval(ta, (t) => ({ v: t.value, pos: t.selectionStart, same: t.dataset.fr6aMark === '1', mode: t.closest('.shell-window').dataset.v4Mode }));
          await F.focus(ta);
          await F.keyboard.press('Escape');
          const esc = await F.$eval(notesSel, (w) => w.dataset.v4Mode);
          await F.keyboard.press('Control+Shift+E');
          const key = await F.$eval(notesSel, (w) => w.dataset.v4Mode);
          await F.keyboard.press('Control+Shift+E');
          const key2 = await F.$eval(notesSel, (w) => w.dataset.v4Mode);
          check('FR6a App ↔ Code im selben Fenster: Entwurf und Cursor bleiben (gleiches Editor-Element), App-Inhalt bleibt montiert; Esc → App, Strg+Umschalt+E schaltet um',
            before.v.includes('fr6a-entwurf') && inApp.mode === 'app' && inApp.appVisible && inApp.codeHidden && back.same && back.v === before.v && back.pos === before.pos && back.mode === 'code'
              && esc === 'app' && key === 'code' && key2 === 'app', JSON.stringify({ before: before.pos, inApp, back: { same: back.same, pos: back.pos, mode: back.mode }, esc, key, key2 }));
          await pickMenu(notesSel, 'code');
        } else check('FR6a Code-Ansicht hat einen Editor (textarea)', false, 'kein textarea');
        // Shell-Tausch: Fenster kommt im Code-Modus zurück.
        await F.evaluate(() => window.__lab.swapShell());
        await F.waitForSelector('[data-v4-ready]');
        const restoredSrc = await F.waitForSelector(`.shell-window[data-window-app="notes"][data-v4-mode="code"] > [data-v4-src-host] [data-v4-source="notes"]`, { timeout: 15000 }).then(() => true, () => false);
        const swapDbg = await F.evaluate(() => [...document.querySelectorAll('.shell-window[data-window-app="notes"]')].map((w) => ({ mode: w.dataset.v4Mode, host: !!w.querySelector('[data-v4-src-host]'), src: !!w.querySelector('[data-v4-source]')})));
        check('FR6a Code-Modus übersteht den Shell-Tausch (gleiches Fenster, kein eigenes Quelltext-Fenster)', restoredSrc && (await F.$$('.shell-window[data-window-view="source"]')).length === 0, JSON.stringify(swapDbg));
        // Ohne apps.source.view: Umschalter weg, Code-Ansicht schließt, openSource → false + Zoneneintrag.
        await F.evaluate(() => window.__lab.kernel.sim.setPermissions([]));
        // Echtes Warten (FR4-5 deterministisch, ZONE45 Runde 2): der Rechte-Spiegel des Rahmens schließt die Code-Ansicht.
        // Keine geschluckte Zeitüberschreitung: bleibt sie offen, scheitert der Lauf hier sichtbar.
        await F.waitForFunction(() => !document.querySelector('[data-v4-src-host]'), null, { timeout: 4000 });
        const r3 = await F.evaluate((o) => {
          const h = window.__lab.frame.shellHandle();
          const w = h.windows().find((r) => r.appId === 'notes');
          return { ok: w.host.instance.api.shell.openSource(), code: !!document.querySelector('[data-v4-src-host]'), mode: w.handle.element.dataset.v4Mode };
        }, zoneOwner);
        // Ohne Recht: Klick auf den Icon-Header zeigt kein „Code ansehen“.
        { const nS = '.shell-window[data-window-app="notes"]'; const wid = await F.$eval(nS, (w) => w.id);
          if (!(await F.$eval(nS, (w) => w.classList.contains('is-focused')))) await F.click(`[data-v4-win="${wid}"]`);
          // Zeitrennen behoben: eine geschlossene Liste behält ihre alten Kinder (früher mitgezählt, wenn der Klick die
          // offene Liste schloss). Jetzt: Liste sicher zu, Icon-Klick, auf die OFFENE Liste warten, nur darin zählen.
          await F.evaluate(() => window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'notes').host.zone.close());
          await F.click(`${nS} .shell-window-v2-icon`, { position: { x: 30, y: 30 } });
          await F.waitForFunction((s) => !!document.querySelector(`${s} .v4-status-zone__list:not([hidden])`), nS, { timeout: 4000 });
          r3.toggleHidden = (await F.$$(`${nS} .v4-status-zone__list:not([hidden]) [data-v4-appmenu="code"], ${nS} .v4-status-zone__list:not([hidden]) [data-v4-appmenu="app"]`)).length === 0; /* FR6b: Melden-Einträge bleiben für alle */ await F.keyboard.press('Escape'); }
        const zoneOwner2 = await F.evaluate(() => window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'notes').ownerId);
        const z3 = await zoneOf(zoneOwner2);
        await F.evaluate((o) => window.__lab.frame.shellHandle().win(o).host.zone.clear('shell.openSource'), zoneOwner2);
        await F.evaluate((p) => window.__lab.kernel.sim.setPermissions(p), perms0);
        // Mutationsbeleg zu „Entwurf und Cursor bleiben“: nach einem echten Schließen (Recht weg) ist das Editor-Element
        // neu (Markierung fehlt) – die Prüfung oben unterscheidet also „erhalten“ von „neu aufgebaut“.
        const nSel = '.shell-window[data-window-app="notes"]';
        await F.waitForTimeout(200);
        await pickMenu(nSel, 'code');
        const fresh = await F.waitForSelector(`${nSel} [data-v4-src-host] textarea`, { timeout: 15000 }).then(() => F.$eval(`${nSel} [data-v4-src-host] textarea`, (t) => t.dataset.fr6aMark !== '1'), () => false);
        const freshDbg = await F.$eval(nSel, (w) => ({ mode: w.dataset.v4Mode, zone: window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'notes').host.zone.entries().map((e) => e.text), host: !!w.querySelector('[data-v4-src-host]'), tas: [...w.querySelectorAll('[data-v4-src-host] textarea')].map((t) => t.dataset.fr6aMark || '-'), srcs: w.querySelectorAll('[data-v4-source]').length }));
        check('FR6a Mutationsprobe: nach echtem Schließen ist der Editor neu (ohne Markierung) – „erhalten“ ist also messbar', fresh, JSON.stringify({ perms0, freshDbg }));
        await pickMenu(nSel, 'app');
        check('FR4-5 ohne apps.source.view: kein „Code ansehen“ im App-Menü, Code-Ansicht schließt, openSource → false und ein Zoneneintrag',
          r3.ok === false && !r3.code && r3.toggleHidden && r3.mode === 'app' && z3.length === 1 && z3[0] === 'error|Quelltext von „notes“ ist nicht verfügbar|Ausblenden', JSON.stringify({ perms0, r3, z3 }));
      }
    }

    // ---- FR4-4: gesperrte Kit-Knöpfe ≥ 3:1 (Text gegen sichtbaren Knopfgrund), hell und dunkel ----
    {
      const contrast = await F.evaluate(async (o) => {
        const root = window.__lab.frame.shellHandle().win(o).host.instance.root;
        const box = document.createElement('div');
        box.dataset.fr4Disabled = '';
        box.innerHTML = ['ctox-button', 'ctox-button is-primary', 'ctox-button ctox-run-control', 'ctox-icon-button', 'ctox-button is-danger']
          .map((c) => `<button type="button" class="${c}" disabled>Gesperrt</button>`).join('');
        root.append(box);
        const parse = (s) => {
          let m = /rgba?\(([^)]+)\)/.exec(s);
          if (m) { const p = m[1].split(/[\s,/]+/).filter(Boolean).map(Number); return [p[0] / 255, p[1] / 255, p[2] / 255, p[3] ?? 1]; }
          m = /color\(srgb ([^)]+)\)/.exec(s);
          if (m) { const p = m[1].split(/[\s/]+/).filter(Boolean).map(Number); return [p[0], p[1], p[2], p[3] ?? 1]; }
          return [0, 0, 0, 0];
        };
        const over = (b, f) => { const a = f[3]; return [0, 1, 2].map((i) => f[i] * a + b[i] * (1 - a)).concat(1); };
        const mix = (a, b, t) => [0, 1, 2].map((i) => a[i] * (1 - t) + b[i] * t).concat(1);
        const lum = (c) => { const l = c.slice(0, 3).map((v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4)); return 0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2]; };
        const ratio = (x, y) => { const a = lum(x), b = lum(y); return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05); };
        const measure = () => [...box.querySelectorAll('button')].map((btn) => {
          const chain = []; for (let n = btn.parentElement; n; n = n.parentElement) chain.unshift(n);
          let under = [1, 1, 1, 1]; let op = 1;
          for (const n of chain) { const cs = getComputedStyle(n); under = over(under, parse(cs.backgroundColor)); op *= Number(cs.opacity); }
          const cs = getComputedStyle(btn);
          const o2 = op * Number(cs.opacity);
          const bg = over(under, parse(cs.backgroundColor));
          const fg = over(bg, parse(cs.color));
          return { cls: btn.className, r: Math.round(ratio(mix(under, fg, o2), mix(under, bg, o2)) * 100) / 100 };
        });
        const html = document.documentElement;
        const prev = html.dataset.theme;
        html.dataset.theme = 'light'; await new Promise((r) => requestAnimationFrame(r));
        const light = measure();
        html.dataset.theme = 'dark'; await new Promise((r) => requestAnimationFrame(r));
        const dark = measure();
        if (prev === undefined) delete html.dataset.theme; else html.dataset.theme = prev;
        box.remove();
        return { light, dark };
      }, zoneOwner);
      const min = Math.min(...contrast.light.map((x) => x.r), ...contrast.dark.map((x) => x.r));
      check('FR4-4 gesperrte Kit-Knöpfe (normal, is-primary, ctox-run-control, Icon, is-danger) ≥ 3:1 in hell und dunkel', min >= 3, JSON.stringify(contrast));
    }

    // ---- FR2-4: zwei Fenster derselben App – eigener root, eigenes api, keine Störung, Shell-Tausch, Schließen ----
    const mw = await F.evaluate(async () => {
      const h = window.__lab.frame.shellHandle();
      const a = h.openWindow('notes', { geometry: { x: 20, y: 20, w: 520, h: 360 } });
      const b = h.openWindow('notes', { geometry: { x: 600, y: 60, w: 520, h: 360 } });
      await Promise.all([h.win(a).host.ready, h.win(b).host.ready]);
      const ra = h.win(a), rb = h.win(b);
      const ia = ra.host.instance.root.querySelector('[data-lab-input]');
      const ib = rb.host.instance.root.querySelector('[data-lab-input]');
      ia.value = 'Fenster A'; ia.dispatchEvent(new Event('input', { bubbles: true }));
      ib.value = 'Fenster B'; ib.dispatchEvent(new Event('input', { bubbles: true }));
      return {
        a, b,
        ownRoot: ra.host.instance.root !== rb.host.instance.root && ra.handle.element.contains(ra.host.instance.root) && rb.handle.element.contains(rb.host.instance.root),
        ownApi: ra.host.instance.api !== rb.host.instance.api && ra.host.instance.api.shell !== rb.host.instance.api.shell,
        values: [ia.value, ib.value],
      };
    });
    await F.evaluate(() => window.__lab.swapShell());
    await F.waitForSelector('[data-v4-ready]');
    await F.waitForTimeout(200);
    const mw2 = await F.evaluate(([a, b]) => {
      const h = window.__lab.frame.shellHandle();
      const v = (o) => h.win(o)?.host.instance?.root.querySelector('[data-lab-input]')?.value ?? null;
      const res = { afterSwap: [v(a), v(b)] };
      h.closeWindow(a);
      return new Promise((r) => setTimeout(() => {
        const rb = h.win(b);
        rb.host.instance.api.shell.setTitle('Notizen B');
        res.afterClose = { aGone: !h.win(a), b: v(b), bState: rb.host.state, bTitle: rb.handle.element.querySelector('.shell-window-title-text').textContent };
        r(res);
      }, 400));
    }, [mw.a, mw.b]);
    check('FR2-4 zwei Fenster derselben App: eigener root und eigenes api-Objekt, Eingaben getrennt',
      mw.ownRoot && mw.ownApi && mw.values.join('|') === 'Fenster A|Fenster B', JSON.stringify(mw));
    check('FR2-4 zwei Fenster überleben Shell-Tausch getrennt; Schließen von A lässt B montiert und bedienbar',
      mw2.afterSwap.join('|') === 'Fenster A|Fenster B' && mw2.afterClose.aGone && mw2.afterClose.b === 'Fenster B' && mw2.afterClose.bState === 'mounted' && mw2.afterClose.bTitle === 'Notizen B', JSON.stringify(mw2));

    if (!STUB) {
      // presentation.multi_instance === false (Apps mit modulweitem Zustand): kein zweites Fenster, vorhandenes nach vorn.
      const solo = await F.evaluate(async () => {
        window.__lab.kernel.sim.activate('ledger', '1.1.0', { app_id: 'ledger', app_ver: '1.1.0', entry: 'index.js', layout: { icon_asset: 'icon.svg', category: 'Finanzen', presentation: { multi_instance: false } } });
        await new Promise((r) => setTimeout(r, 300));
        const h = window.__lab.frame.shellHandle();
        const a = h.openWindow('ledger');
        await h.win(a).host.ready;
        const b = h.openWindow('ledger');
        const n = h.windows().filter((r) => r.appId === 'ledger').length;
        h.closeWindow(a);
        return { same: a === b, n };
      });
      check('FR2-4 multi_instance:false im Layout verhindert ein zweites Fenster', solo.same && solo.n === 1, JSON.stringify(solo));
    }

    // ---- FR2-6 / FR2-5: Launcher-Reihenfolge per Ziehen und Tastatur; Anheften/Lösen per Ziehen ----
    await F.evaluate(() => { for (const r of window.__lab.frame.shellHandle().windows()) window.__lab.frame.shellHandle().closeWindow(r.ownerId); });
    await F.waitForTimeout(500);
    const arbeit = () => F.$$eval('[data-v4-group="Arbeit"] [data-v4-app]', (els) => els.map((e) => e.dataset.v4App).join(','));
    const order0 = await arbeit();
    await startOpen(F);
    await F.dragAndDrop('[data-v4-app="notes"]', '[data-v4-app="tasks"]', { targetPosition: { x: 40, y: 4 } }); // FR6d: Liste untereinander, obere Hälfte = davor
    await F.waitForTimeout(100);
    const order1 = await arbeit();
    await F.evaluate(() => window.__lab.frame.shellHandle().preferencesWritten());
    const prefOrder = await F.evaluate((stub) => (stub
      ? window.__lab.kernel.appApi('shell', 'dev').get('shell_preferences', 'owner').get()?.launcher_order
      : window.__lab.kernel.sim.prefs.get('shell_preferences')?.launcher_order), STUB);
    await startOpen(F);
    await F.focus('[data-v4-app="notes"] [data-v4-open]');
    await F.keyboard.press('Alt+ArrowRight');
    await F.waitForTimeout(80);
    const order2 = await arbeit();
    const focusKept = await F.evaluate(() => document.activeElement?.closest('[data-v4-app]')?.dataset.v4App);
    check('FR2-6 Launcher-Reihenfolge per Ziehen (Gruppe Arbeit) und Alt+Pfeil, gespeichert in shell_preferences',
      order0 === 'tasks,notes' && order1 === 'notes,tasks' && Array.isArray(prefOrder) && prefOrder.indexOf('notes') < prefOrder.indexOf('tasks') && order2 === 'tasks,notes' && focusKept === 'notes',
      JSON.stringify({ order0, order1, prefOrder, order2, focusKept }));
    await startOpen(F);
    await F.dragAndDrop('[data-v4-app="ledger"]', '[data-module-tabs]', { targetPosition: { x: 200, y: 20 } });
    await startOpen(F);
    await F.dragAndDrop('[data-v4-app="crm"]', '[data-module-tabs] [data-v4-pin="ledger"]');
    await F.waitForTimeout(80);
    const pins1 = await F.$$eval('[data-module-tabs] [data-v4-pin]', (els) => els.map((e) => e.dataset.v4Pin).join(','));
    await F.keyboard.press('Escape');
    const lb = await (await F.$('[data-v4-desktop]')).boundingBox(); // FR6d: „Fläche“ = Desktop
    await F.dragAndDrop('[data-module-tabs] [data-v4-pin="ledger"]', '[data-v4-desktop]', { targetPosition: { x: lb.width - 20, y: lb.height - 20 } });
    await F.waitForTimeout(80);
    const pins2 = await F.$$eval('[data-module-tabs] [data-v4-pin]', (els) => els.map((e) => e.dataset.v4Pin).join(','));
    check('FR2-5 Anheften per Ziehen auf die Taskleiste (an Position), Lösen per Ziehen auf die Fläche',
      pins1 === 'crm,ledger' && pins2 === 'crm' && JSON.stringify((await F.evaluate(() => window.__lab.frame.describeShell().pins))) === '["crm"]', `${pins1} → ${pins2}`);
    await F.screenshot({ path: shot('fr2-launcher-1280.png') });
    await F.close();
  }

  check('Keine Seitenfehler', errors.length === 0, errors.join(' | '));

  if (STUB) {
    // --- Pins + Launcher-Reihenfolge ueberleben Shell-Tausch und neuen Tab (shell_preferences, §16.19 Nr. 3) ---
    // Ein Browser-Kontext, IndexedDB, Mehr-Tab: zweiter Tab = neuer tabKey, also KEIN window_state/Snapshot.
    const ctx2 = await browser.newContext({ viewport: { width: 1280, height: 800 } });
    const perr = [];
    const inst = `pins-${Date.now().toString(36)}`;
    const labUrl = `${base}/v4/frame/lab/stub-lab.html?storage=indexeddb&multi=1&inst=${inst}`;
    const openTab = async () => {
      const p = await ctx2.newPage();
      p.on('pageerror', (e) => perr.push(String(e)));
      p.on('console', (m) => { if (m.type() === 'error') perr.push(m.text()); });
      await p.goto(labUrl);
      await p.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'), null, { timeout: 20000 });
      await p.waitForFunction(() => document.querySelectorAll('[data-v4-launcher] [data-v4-app] img').length === 3, null, { timeout: 20000 });
      return p;
    };
    const P1 = await openTab();
    await startOpen(P1);
    for (const id of ['tasks', 'ledger']) {
      await P1.hover(`[data-v4-app="${id}"]`);
      await P1.click(`[data-v4-app="${id}"] [data-v4-pin]`);
    }
    await P1.evaluate(() => window.__lab.frame.shellHandle().setLauncherOrder(['tasks', 'notes']));
    await P1.evaluate(() => window.__lab.frame.shellHandle().preferencesWritten());
    const arbeit = (p) => p.$$eval('[data-v4-group="Arbeit"] [data-v4-app]', (els) => els.map((e) => e.dataset.v4App).join(','));
    const pinsOf = (p) => p.$$eval('[data-module-tabs] [data-v4-pin]', (els) => els.map((e) => e.dataset.v4Pin).sort().join(','));
    const p1Before = { pins: await pinsOf(P1), order: await arbeit(P1) };
    const sw = await P1.evaluate(() => window.__lab.swapShell().then((r) => ({ swapped: r.swapped, error: r.error || '' })));
    await P1.waitForSelector('[data-v4-ready]');
    const p1After = { pins: await pinsOf(P1), order: await arbeit(P1) };
    check('Pins + Launcher-Reihenfolge ueberleben Shell-Tausch', sw.swapped && p1Before.pins === 'ledger,tasks' && p1After.pins === p1Before.pins && p1After.order === 'tasks,notes',
      JSON.stringify({ sw, p1Before, p1After }));
    const P2 = await openTab();
    await P2.waitForFunction(() => document.querySelectorAll('[data-module-tabs] [data-v4-pin]').length === 2, null, { timeout: 10000 }).catch(() => {});
    const p2 = await P2.evaluate(async () => ({
      windowState: await window.__lab.kernel.windowState.get('shell'),
      row: window.__lab.kernel.appApi('shell', 'dev').get('shell_preferences', 'owner').get(),
      isLeader: window.__lab.kernel.isLeader,
    }));
    const p2View = { pins: await pinsOf(P2), order: await arbeit(P2) };
    check('Neuer Tab: Pins + Reihenfolge aus shell_preferences (ohne window_state)', p2.windowState === null && !p2.isLeader && p2View.pins === 'ledger,tasks' && p2View.order === 'tasks,notes',
      JSON.stringify({ p2View, windowState: p2.windowState, isLeader: p2.isLeader, row: p2.row }));
    // Live: Tab 2 loest einen Pin, Tab 1 folgt ohne Reload.
    await P2.evaluate(async () => {
      const app = document.querySelector('[data-v4-app="ledger"]');
      app.querySelector('[data-v4-pin]').click();
      await window.__lab.frame.shellHandle().preferencesWritten();
    });
    const live = await P1.waitForFunction(() => !document.querySelector('[data-v4-pin="ledger"]') && !!document.querySelector('[data-v4-pin="tasks"]'), null, { timeout: 8000 }).then(() => true, () => false);
    check('Pin-Aenderung in Tab 2 erscheint live in Tab 1', live, await pinsOf(P1));
    check('Pin-Test ohne Seitenfehler', perr.length === 0, perr.join(' | '));
    await ctx2.close();
  }

  // --- FR3-2: Kategorie-Gruppen nach normalisiertem Schlüssel (Groß/Klein, Leerraum) ---
  {
    const ctx3 = await browser.newContext({ viewport: { width: 1280, height: 800 } });
    const G = await ctx3.newPage();
    const gerr = [];
    G.on('pageerror', (e) => gerr.push(String(e)));
    await G.goto(`${base}/v4/frame/lab/${STUB ? 'stub-lab' : 'index'}.html`);
    await G.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
    await G.evaluate(async (stub) => {
      const add = (id, title, category) => (stub
        ? window.__lab.kernel.sim.publishAppVersion(id, '1.0.0', { title, layout: { category }, styles: [] })
        : window.__lab.kernel.sim.addApp({ id, title, active_ver: '1.0.0', activated_ver: '1.0.0', state: 'activated', target_ver: null, error: null },
          { app_id: id, app_ver: '1.0.0', entry: 'index.js', layout: { category } }));
      await add('kleinarbeit', 'Klein', 'arbeit');
      await add('grossarbeit', 'Gross', '  ARBEIT ');
      await add('dev1', 'Dev Eins', 'development');
      await add('dev2', 'Dev Zwei', 'Development');
      await add('dev3', 'Dev Drei', 'Development');
    }, STUB);
    await G.waitForFunction(() => document.querySelectorAll('[data-v4-launcher] [data-v4-app]').length === 8, null, { timeout: 8000 });
    await startOpen(G);
    const groupsSeen = await G.$$eval('[data-v4-group]', (els) => els.map((e) => ({ label: e.dataset.v4Group, head: e.querySelector('[data-v4-group-head]').textContent, n: e.querySelectorAll('[data-v4-app]').length })));
    const upper = await G.$$eval('[data-v4-group] [data-v4-group-head]', (els) => els.map((e) => e.innerText));
    // FR5-3: englische Rohwerte (development/Development) erscheinen über das Glossar deutsch („Entwicklung“).
    check('FR3-2 Kategorie-Gruppen nach normalisiertem Schlüssel: eine Gruppe je Kategorie, Überschrift deutsch aus dem Glossar',
      JSON.stringify(groupsSeen) === JSON.stringify([{ label: 'Arbeit', head: 'Arbeit', n: 4 }, { label: 'Entwicklung', head: 'Entwicklung', n: 3 }, { label: 'Finanzen', head: 'Finanzen', n: 1 }])
        && new Set(upper).size === upper.length, JSON.stringify({ groupsSeen, upper }));
    await startOpen(G);
    await G.fill('[data-v4-search]', 'dev');
    const searchGroups = await G.$$eval('[data-v4-group]', (els) => els.map((e) => `${e.dataset.v4Group}:${e.querySelectorAll('[data-v4-app]').length}`).join(','));
    check('FR3-2 Suche über die Gruppenbezeichnung findet alle Schreibweisen', searchGroups === 'Entwicklung:3', searchGroups);
    await G.fill('[data-v4-search]', 'entwickl');
    const searchDe = await G.$$eval('[data-v4-group]', (els) => els.map((e) => `${e.dataset.v4Group}:${e.querySelectorAll('[data-v4-app]').length}`).join(','));
    const headCase = await G.$eval('[data-v4-group] [data-v4-group-head]', (e) => getComputedStyle(e).textTransform);
    check('FR5-3 Glossar: Suche nach dem deutschen Kategoriebegriff, Überschrift wie Shell-V2 in Versalien (Owner 15.10.: bestehende Shell bleibt)', searchDe === 'Entwicklung:3' && headCase === 'uppercase', `${searchDe} / ${headCase}`);
    if (!STUB) check('FR3-1 Ohne kernel.source und apps.source.edit kein Einstieg „Neue App“', (await G.$$('[data-v4-launcher-action]')).length === 0);
    check('FR3-2 ohne Seitenfehler', gerr.length === 0, gerr.join(' | '));
    await ctx3.close();
  }

  // --- FR3-1/3: „Neue App“ im Launcher (Admin), Ordner-Upload einer Fremd-App, Icon ohne xmlns ---
  if (STUB) {
    const ctx4 = await browser.newContext({ viewport: { width: 1280, height: 800 } });
    const N = await ctx4.newPage();
    const nerr = [];
    N.on('pageerror', (e) => nerr.push(String(e)));
    N.on('console', (m) => { if (m.type() === 'error' && !/404/.test(m.text())) nerr.push(m.text()); });
    await N.goto(`${base}/v4/frame/lab/stub-lab.html?inst=newapp-${Date.now().toString(36)}`);
    await N.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
    await startOpen(N); // FR6d: „Neue App“ sitzt im Kopf des Startmenüs
    const action = await N.$('[data-v4-launcher] [data-v4-launcher-action="new-app"]');
    const actionText = action ? (await action.innerText()).trim() : '';
    check('FR3-1 Admin (apps.source.edit) sieht „Neue App“ im Launcher', !!action && actionText === 'Neue App' && await action.isVisible(), actionText);
    await N.click('[data-v4-launcher-action="new-app"]');
    await N.waitForSelector('.shell-window[data-window-view="new-app"] [data-v4-source-upload]');
    await N.waitForTimeout(600);
    const placed = await N.evaluate(() => {
      const ui = document.querySelector('[data-v4-source-upload]');
      const win = ui.closest('.shell-window');
      const frame = document.querySelector('[data-v4-frame]');
      const outside = [...document.body.children].filter((e) => e.id !== 'ctox-shell' && e.tagName !== 'SCRIPT');
      return { inWindow: !!win && win.dataset.windowView === 'new-app', inFrame: frame.contains(ui), outside: outside.map((e) => e.tagName), title: win?.querySelector('.shell-window-title-text')?.textContent, task: document.querySelectorAll('[data-v4-win]').length };
    });
    check('FR3-1 „Neue App“ öffnet den Quelltext-Modus (Neue-App-Fläche) als Fenster im Rahmen, kein Overlay',
      placed.inWindow && placed.inFrame && placed.outside.length === 0 && placed.title === 'Neue App' && placed.task === 1, JSON.stringify(placed));
    await N.screenshot({ path: shot('fr3-new-app-1280.png') });
    await N.setInputFiles('[data-v4-source-upload] input[type=file]', path.join(here, 'upload/fremd'));
    await N.waitForSelector('[data-v4-source-upload] [data-files] li');
    await N.screenshot({ path: shot('fr3-new-app-folder-1280.png') });
    const listed = await N.locator('[data-v4-source-upload] [data-files] li').allTextContents();
    const appIdVal = await N.inputValue('[data-v4-source-upload] [data-app-id]');
    await N.click('[data-v4-source-upload] [data-publish]');
    const appeared = await N.waitForSelector('[data-v4-launcher] [data-v4-app="fremd"]', { state: 'attached', timeout: 8000 }).then(() => true, () => false);
    const pub = await N.evaluate(async () => {
      const k = window.__lab.kernel;
      const row = k.apps.get().find((a) => a.id === 'fremd');
      const files = row ? await k.source('fremd').list(row.active_ver) : [];
      return { row: row ? { active_ver: row.active_ver, title: row.title } : null, files, last: document.querySelector('[data-v4-source-upload]')?.dataset.lastPublished || '', zone: document.querySelector('.shell-window[data-window-view="new-app"] [data-v4-zone-slot]')?.innerText.trim() || '' };
    });
    check('FR3-1 Ordner-Upload über „Neue App“ veröffentlicht eine neue App (base null), Kachel erscheint live im Launcher',
      appeared && appIdVal === 'fremd' && listed.includes('icon.svg') && pub.row?.title === 'Fremde App' && pub.files.includes('icon.svg') && pub.last.startsWith('fremd@') && pub.zone === '',
      JSON.stringify({ appeared, appIdVal, listed, pub }));
    const groupOf = await N.$eval('[data-v4-app="fremd"]', (t) => t.closest('[data-v4-group]').dataset.v4Group);
    const groupsN = await N.$$eval('[data-v4-group]', (els) => els.map((e) => e.dataset.v4Group).join(','));
    check('FR3-2 Fremd-App mit Kategorie „arbeit“ landet in der bestehenden Gruppe „Arbeit“', groupOf === 'Arbeit' && groupsN === 'Arbeit,Finanzen', `${groupOf} / ${groupsN}`);
    const icon = await N.waitForFunction(() => {
      const img = document.querySelector('[data-v4-app="fremd"] img');
      return img && 'v4IconRepaired' in img.dataset && img.complete && img.naturalWidth > 0 ? { src: img.getAttribute('src').slice(0, 40), w: img.naturalWidth } : null;
    }, null, { timeout: 8000 }).then((h) => h.jsonValue(), () => null);
    const glyph = await N.$('[data-v4-app="fremd"] .v4f-icon.is-glyph');
    check('FR3-3 Icon ohne xmlns (icon_asset) wird beim Einlesen ergänzt und angezeigt', !!icon && icon.src.startsWith('data:image/svg+xml') && !glyph, JSON.stringify(icon));
    const svgInline = await N.evaluate(async () => {
      const mod = await import('/v4/frame/index.js');
      return [mod.ensureSvgXmlns('<svg viewBox="0 0 1 1"></svg>'), mod.ensureSvgXmlns('<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"/>'), mod.ensureSvgXmlns('kein svg')];
    });
    check('FR3-3 ensureSvgXmlns: ergänzt nur fehlendes xmlns', svgInline[0] === '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"></svg>' && svgInline[1] === '<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"/>' && svgInline[2] === 'kein svg', JSON.stringify(svgInline));
    await N.evaluate(() => window.__lab.kernel.sim.publishAppVersion('inlineicon', '1.0.0', { title: 'Inline', layout: { icon_svg: '<svg viewBox="0 0 10 10"><rect width="10" height="10" fill="#0a7"/></svg>', category: 'Arbeit' }, styles: [] }));
    const inlineIcon = await N.waitForFunction(() => {
      const img = document.querySelector('[data-v4-app="inlineicon"] img');
      return img && img.complete ? { ns: decodeURIComponent(img.getAttribute('src')).includes('xmlns="http://www.w3.org/2000/svg"'), w: img.naturalWidth } : null;
    }, null, { timeout: 8000 }).then((h) => h.jsonValue(), () => null);
    check('FR3-3 Icon ohne xmlns (icon_svg) wird beim Einlesen ergänzt', !!inlineIcon && inlineIcon.ns && inlineIcon.w > 0, JSON.stringify(inlineIcon));
    await N.screenshot({ path: shot('fr3-new-app-published-1280.png') });
    await N.setViewportSize({ width: 420, height: 800 });
    await N.waitForTimeout(400);
    await N.screenshot({ path: shot('fr3-new-app-window-420.png') });
    await N.click('[data-module-tabs] [data-v4-win]'); // Ansicht minimieren, Launcher sichtbar
    await N.waitForTimeout(500);
    await startOpen(N);
    const narrow = await N.evaluate(() => {
      const a = document.querySelector('[data-v4-launcher-action="new-app"]').getBoundingClientRect();
      const s = document.querySelector('[data-v4-search]').getBoundingClientRect();
      return { overflow: document.documentElement.scrollWidth > innerWidth, actionRight: Math.round(a.right), search: Math.round(s.width), sameRow: Math.abs(a.top - s.top) < 6 };
    });
    check('FR3-1 420 px: Suche und „Neue App“ in einer Zeile, kein waagrechter Überlauf', !narrow.overflow && narrow.actionRight <= 420 && narrow.search >= 150 && narrow.sameRow, JSON.stringify(narrow));
    await N.screenshot({ path: shot('fr3-new-app-420.png') });
    await N.keyboard.press('Escape');
    await N.click('[data-module-tabs] [data-v4-win]');
    await N.setViewportSize({ width: 1280, height: 800 });
    // Nicht-Admin: Recht entzogen → Einstieg weg, offene Ansicht geschlossen; nach Shell-Tausch bleibt er weg.
    await N.evaluate(() => window.__lab.kernel.sim.setPermissions([]));
    await N.waitForFunction(() => !document.querySelector('[data-v4-launcher-action]') && !document.querySelector('.shell-window[data-window-view="new-app"]'), null, { timeout: 4000 }).catch(() => {});
    const after = await N.evaluate(() => ({ action: !!document.querySelector('[data-v4-launcher-action]'), win: !!document.querySelector('.shell-window[data-window-view="new-app"]') }));
    const sw = await N.evaluate(() => window.__lab.swapShell().then((r) => ({ swapped: r.swapped, error: r.error || '' })));
    await N.waitForSelector('[data-v4-ready]');
    const afterSwap = await N.evaluate(() => ({ action: !!document.querySelector('[data-v4-launcher-action]'), viaApi: window.__lab.frame.shellHandle().openNewApp() }));
    check('FR3-1 Nicht-Admin (ohne apps.source.edit) sieht keinen Einstieg, offene Ansicht schließt, auch nach Shell-Tausch',
      !after.action && !after.win && sw.swapped && !afterSwap.action && afterSwap.viaApi === null, JSON.stringify({ after, sw, afterSwap }));
    await N.screenshot({ path: shot('fr3-nonadmin-1280.png') });
    // Wieder Admin: Einstieg kommt live zurück; Ansicht überlebt einen Shell-Tausch (Snapshot).
    await N.evaluate(() => window.__lab.kernel.sim.setPermissions(['apps.source.view', 'apps.source.edit']));
    await N.waitForSelector('[data-v4-launcher-action="new-app"]', { state: 'attached', timeout: 4000 });
    await startOpen(N);
    await N.click('[data-v4-launcher-action="new-app"]');
    await N.waitForSelector('.shell-window[data-window-view="new-app"] [data-v4-source-upload]');
    await N.evaluate(() => window.__lab.swapShell());
    await N.waitForSelector('[data-v4-ready]');
    const restoredView = await N.waitForSelector('.shell-window[data-window-view="new-app"] [data-v4-source-upload]', { timeout: 5000 }).then(() => true, () => false);
    check('FR3-1 Ansicht „Neue App“ übersteht den Shell-Tausch', restoredView && (await N.$$('.shell-window[data-window-view="new-app"]')).length === 1);
    check('FR3 ohne Seitenfehler', nerr.length === 0, nerr.join(' | '));
    await ctx4.close();
  }
  // ---- Befehls-Lebenszyklus in der Zone (§16.31, Endzustände nach business-command-lifecycle-v2.json) ----
  // Invarianten: (1) jeder Endzustand höchstens einmal je Zone (Wiederzustellung, Remount, Shell-Tausch, zweiter Tab),
  // (2) kein Rückwärtsübergang, (3) Freigabe in client_seq-Folge, (4) done ist still und räumt keinen fremden Fehler.
  {
    const cctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
    const cerr = [];
    const cmdUrl = STUB ? `${base}/v4/frame/lab/stub-lab.html?storage=indexeddb&multi=1&inst=cmd-${Date.now().toString(36)}` : `${base}/v4/frame/lab/index.html?cmd=1`;
    const openTab = async () => {
      const p = await cctx.newPage();
      p.on('pageerror', (e) => cerr.push(String(e)));
      p.on('console', (m) => { if (m.type() === 'error') cerr.push(m.text()); });
      await p.goto(cmdUrl);
      await p.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'), null, { timeout: 20000 });
      await p.waitForFunction(() => document.querySelectorAll('[data-v4-launcher] [data-v4-app] img').length === 3, null, { timeout: 20000 });
      await startOpen(p);
      await p.fill('[data-v4-search]', 'Buch');
      await p.press('[data-v4-search]', 'Enter');
      await p.waitForSelector('.shell-window[data-window-app="ledger"] [data-lab-app="ledger"]');
      // Labor-Hilfen: Befehle der App ledger auslösen und Endzustände zustellen (Stub: Gate je Befehl im Core).
      await p.evaluate((stub) => {
        const host = () => window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'ledger').host;
        const gates = {};
        const gate = (k) => gates[k] || (gates[k] = (() => { let open; const pr = new Promise((r) => { open = r; }); pr.open = open; return pr; })());
        const handles = {};
        if (stub) window.__lab.kernel.sim.onCommand('lab.cmd', async (pl) => { await gate(pl.k); if (pl.fail) throw new Error(pl.fail); return { ok: true }; });
        window.__cmd = {
          issue(k, { fail = null, record_id } = {}) {
            const h = host().instance.api.command('lab.cmd', { k, fail }, record_id ? { record_id } : {});
            handles[k] = { h, fail };
            return h.id;
          },
          cancel: (k) => handles[k].h.cancel(),
          // Endzustand zustellen: Stub = Gate öffnen (Core führt aus), Attrappe = Ergebnis + Statusdokument.
          finish(k, result = { status: 'accepted' }) {
            const { h, fail } = handles[k];
            if (stub) { gate(k).open(); return; }
            window.__lab.kernel.sim.cmd.result(h.id, result);
            const rejected = result.status === 'rejected';
            window.__lab.kernel.sim.cmd.deliver(h.id, fail || rejected ? { status: 'failed', error: rejected ? result.reason : fail } : { status: 'done', progress: 1, result: { ok: true } });
          },
          // Statusdokument (erneut / veraltet) zustellen.
          async redeliver(k, state) {
            const { h } = handles[k];
            if (stub) await window.__lab.kernel.sim.push('business_commands', [{ id: h.id, type: 'lab.cmd', app_id: 'ledger', ...state }]);
            else window.__lab.kernel.sim.cmd.deliver(h.id, state);
          },
          state: (k) => handles[k].h.status.get(),
          id: (k) => handles[k].h.id,
          entries: () => host().zone.entries().filter((e) => /command:|write\.rejected/.test(e.code)).map((e) => ({ code: e.code, text: e.text, level: e.level })),
          line: () => document.querySelector('.shell-window[data-window-app="ledger"] .v4-status-zone__line')?.textContent || '',
        };
      }, STUB);
      return p;
    };
    const C = await openTab();
    const C2 = await openTab(); // zweiter Tab derselben Instanz (Stub: Mehr-Tab über IndexedDB)
    await C.bringToFront();
    const ent = (p = C) => p.evaluate(() => window.__cmd.entries());
    const forId = (list, id) => list.filter((e) => e.code.endsWith(`command:${id}`));
    const settle = (ms = 250) => C.waitForTimeout(ms);
    const waitEntry = (id, p = C) => p.waitForFunction((i) => window.__cmd.entries().some((e) => e.code.endsWith(`command:${i}`)), id, { timeout: 8000 });

    // (1) Endzustand höchstens einmal: Fehler, Wiederzustellung ×2, Remount, Shell-Tausch, zweiter Tab.
    const idA = await C.evaluate(() => window.__cmd.issue('a', { fail: 'kaputt', record_id: 'r1' }));
    await C.evaluate(() => window.__cmd.finish('a'));
    await waitEntry(idA);
    const a0 = forId(await ent(), idA);
    const failedA = await C.evaluate(() => window.__cmd.state('a'));
    await C.evaluate((s) => window.__cmd.redeliver('a', s), failedA);
    await C.evaluate((s) => window.__cmd.redeliver('a', s), failedA);
    await settle();
    const a1 = forId(await ent(), idA);
    await C.evaluate((stub) => (stub ? window.__lab.publish('ledger', '1.1.0', 'Buchhaltung') : window.__lab.kernel.sim.activate('ledger', '1.1.0', window.__lab.manifestFor('ledger', '1.1.0', 'Finanzen'))), STUB);
    await C.waitForSelector('.shell-window[data-window-app="ledger"] [data-lab-ver="1.1.0"]');
    await settle();
    const a2 = forId(await ent(), idA);
    const swA = await C.evaluate(() => window.__lab.swapShell().then((r) => ({ swapped: r.swapped !== false, error: r.error || '' })));
    await C.waitForSelector('.shell-window[data-window-app="ledger"] [data-lab-app="ledger"]');
    await settle();
    const a3 = forId(await ent(), idA);
    const a3tab2 = forId(await ent(C2), idA);
    check('Befehl (1): Fehler-Endzustand genau einmal – nach 2× Wiederzustellung, Remount, Shell-Tausch; zweiter Tab höchstens einmal',
      a0.length === 1 && a0[0].level === 'error' && a0[0].text.includes('kaputt') && a1.length === 1 && a2.length === 1 && a3.length === 1 && swA.swapped && a3tab2.length <= 1,
      JSON.stringify({ a0: a0.length, a1: a1.length, a2: a2.length, a3: a3.length, tab2: a3tab2.length, swA, text: a0[0]?.text }));

    // (1) Ablehnung und Rücknahme: je höchstens ein Eintrag (Ablehnung zeigt der Kernel selbst, sonst die Shell).
    const before = (await ent()).length;
    if (STUB) await C.evaluate(() => window.__lab.kernel.sim.rejectNext('forbidden'));
    const idB = await C.evaluate(() => window.__cmd.issue('b', { record_id: 'r2' }));
    await C.evaluate(() => window.__cmd.finish('b', { status: 'rejected', reason: 'forbidden' }));
    await C.waitForFunction((n) => window.__cmd.entries().length > n, before, { timeout: 8000 });
    await C.waitForFunction(() => window.__cmd.state('b')?.status === 'failed', null, { timeout: 8000 });
    await settle(400);
    const afterB = await ent();
    const newB = afterB.length - before;
    const rejTexts = afterB.slice(before).map((e) => e.text);
    // Rücknahme synchron direkt nach dem Auslösen (vor dem Senden); die Attrappe kennt kein cancel → Ergebnis zustellen.
    const { idX, cancelled } = await C.evaluate(() => { const id = window.__cmd.issue('x'); return { idX: id, cancelled: window.__cmd.cancel('x') }; });
    if (!STUB) await C.evaluate(() => window.__cmd.finish('x', { status: 'rejected', reason: 'cancelled' }));
    await C.waitForFunction(() => window.__cmd.state('x')?.status === 'failed', null, { timeout: 8000 });
    await settle(400);
    const afterX = await ent();
    check('Befehl (1): Ablehnung genau ein Eintrag, Rücknahme (cancelled) still',
      newB === 1 && rejTexts.every((t) => /forbidden/.test(t)) && (cancelled || !STUB) && afterX.length === afterB.length && forId(afterX, idX).length === 0,
      JSON.stringify({ newB, rejTexts, cancelled, afterX: afterX.length, afterB: afterB.length, idB }));

    // (2) Kein Rückwärtsübergang: running nach done ändert nichts, veraltetes failed nach done auch nicht;
    //     running/done nach failed lässt den Fehler stehen.
    const idC = await C.evaluate(() => window.__cmd.issue('c'));
    await C.evaluate(() => window.__cmd.finish('c'));
    await C.waitForFunction(() => window.__cmd.state('c')?.status === 'done', null, { timeout: 8000 });
    const idD = await C.evaluate(() => window.__cmd.issue('d', { fail: 'D-Fehler' }));
    await C.evaluate(() => window.__cmd.finish('d'));
    await waitEntry(idD);
    await settle();
    const z0 = { entries: await ent(), line: await C.evaluate(() => window.__cmd.line()) };
    await C.evaluate(() => window.__cmd.redeliver('c', { status: 'running', progress: 0.5 }));
    await C.evaluate(() => window.__cmd.redeliver('c', { status: 'failed', error: 'veraltet' }));
    await C.evaluate(() => window.__cmd.redeliver('d', { status: 'running', progress: 0.5 }));
    await C.evaluate(() => window.__cmd.redeliver('d', { status: 'done', progress: 1 }));
    await settle(400);
    const z1 = { entries: await ent(), line: await C.evaluate(() => window.__cmd.line()) };
    // Nachweis, dass die veralteten Dokumente die App wirklich erreicht haben (sonst wäre die Prüfung leer).
    const arrived = await C.evaluate(() => [window.__cmd.state('c')?.status, window.__cmd.state('d')?.status].join(','));
    check('Befehl (2): kein Rückwärtsübergang (running/failed nach done, running/done nach failed ändern die Zone nicht)',
      arrived === 'failed,done' && JSON.stringify(z0) === JSON.stringify(z1) && forId(z1.entries, idC).length === 0 && forId(z1.entries, idD).length === 1,
      JSON.stringify({ arrived, z0: z0.entries.length, z1: z1.entries.length, l0: z0.line, l1: z1.line, c: forId(z1.entries, idC).length, d: forId(z1.entries, idD).length }));

    // (3) client_seq-Folge: e1 (n) und e2 (n+1) scheitern, Statusdokumente vertauscht → Zone zeigt e1 vor e2.
    const idE1 = await C.evaluate(() => window.__cmd.issue('e1', { fail: 'eins' }));
    const idE2 = await C.evaluate(() => window.__cmd.issue('e2', { fail: 'zwei' }));
    await C.evaluate(() => window.__cmd.finish('e2'));
    await C.waitForFunction(() => window.__cmd.state('e2')?.status === 'failed', null, { timeout: 8000 });
    await settle(300);
    const held = forId(await ent(), idE2).length;
    await C.evaluate(() => window.__cmd.finish('e1'));
    await waitEntry(idE1);
    await waitEntry(idE2);
    const e = await ent();
    const posE1 = e.findIndex((x) => x.code.endsWith(`command:${idE1}`));
    const posE2 = e.findIndex((x) => x.code.endsWith(`command:${idE2}`));
    const lineE = await C.evaluate(() => window.__cmd.line());
    check('Befehl (3): vertauschte Statusdokumente – n+1 wartet auf n, Zone schließt n vor n+1 ab',
      held === 0 && posE1 >= 0 && posE2 > posE1 && /zwei/.test(lineE),
      JSON.stringify({ held, posE1, posE2, line: lineE }));

    // (4) done auf demselben Ziel räumt den Fehler nicht auf und erzeugt keinen Erfolgstext.
    const idF1 = await C.evaluate(() => window.__cmd.issue('f1', { fail: 'Ziel-Fehler', record_id: 'r9' }));
    await C.evaluate(() => window.__cmd.finish('f1'));
    await waitEntry(idF1);
    await settle();
    const f0 = { entries: await ent(), line: await C.evaluate(() => window.__cmd.line()) };
    const idF2 = await C.evaluate(() => window.__cmd.issue('f2', { record_id: 'r9' }));
    await C.evaluate(() => window.__cmd.finish('f2'));
    await C.waitForFunction(() => window.__cmd.state('f2')?.status === 'done', null, { timeout: 8000 });
    await settle(400);
    const f1 = { entries: await ent(), line: await C.evaluate(() => window.__cmd.line()) };
    const allTexts = await C.evaluate(() => window.__lab.frame.shellHandle().windows().find((r) => r.appId === 'ledger').host.zone.entries().map((x) => x.text));
    check('Befehl (4): done auf demselben Ziel – Fehler bleibt, kein Erfolgs-Zustandstext',
      JSON.stringify(f0) === JSON.stringify(f1) && forId(f1.entries, idF1).length === 1 && forId(f1.entries, idF2).length === 0 && !allTexts.some((t) => /erfolgreich|erledigt|abgeschlossen|done/i.test(t)),
      JSON.stringify({ f0: f0.entries.length, f1: f1.entries.length, line: f1.line, allTexts }));
    await C.screenshot({ path: shot('command-zone-1280.png') });
    check('Befehl: ohne Seitenfehler', cerr.length === 0, cerr.join(' | '));
    await cctx.close();
  }

  // ================= FR6a: schwebendes Chrome, reservierte Zonen, Andock-Raster, Icons aus einer Quelle, Kanten (G8/G9), Vollbild =================
  {
    const ctx6 = await browser.newContext({ viewport: { width: 1280, height: 800 } });
    const P = await ctx6.newPage();
    const perr = [];
    P.on('pageerror', (e) => perr.push(String(e)));
    P.on('console', (m) => { if (m.type() === 'error' && !/404|Failed to load resource/.test(m.text())) perr.push(m.text()); });
    const open6 = async (noKit = false) => {
      await P.goto(`${base}/v4/frame/lab/${STUB ? `stub-lab.html?inst=fr6a-${Date.now().toString(36)}` : 'index.html'}`);
      await P.waitForFunction(() => window.__labReady === true && document.querySelector('[data-v4-ready]'));
      await P.evaluate(async ([stub, noKit]) => {
        globalThis.__labStudioNoKit = noKit;
        const crmSvg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" fill="#7b5cc4"/><text x="32" y="42" font-family="system-ui,sans-serif" font-size="28" font-weight="600" fill="#fff" text-anchor="middle">K</text></svg>';
        const add = (id, title, layout, ver = '1.0.0') => (stub
          ? window.__lab.kernel.sim.publishAppVersion(id, ver, { title, layout, styles: [] })
          : window.__lab.kernel.sim.addApp({ id, title, active_ver: ver, activated_ver: ver, state: 'activated', target_ver: null, error: null },
            { app_id: id, app_ver: ver, entry: 'index.js', layout }));
        // Absichtlich falsches frame_palette: darf die Kanten NICHT beeinflussen (M-17).
        await add('studio', 'Black Hole Studio', { category: 'Arbeit', frame_palette: { start: '#00ff00', middle: '#00ff00', end: '#00ff00', top_joint: '#00ff00', left_joint: '#00ff00' }, default_width: 900, default_height: 520 });
        await add('crm', 'Kontakte', { category: 'Vertrieb', icon_svg: crmSvg, default_width: 900, default_height: 520 }, '2.0.0');
      }, [STUB, noKit]);
      await P.waitForSelector('[data-v4-app="studio"] img', { state: 'attached' });
      const ids = {};
      for (const [id, g] of [['notes', { x: 20, y: 20, w: 560, h: 360 }], ['crm', { x: 600, y: 20, w: 640, h: 360 }], ['studio', { x: 200, y: 200, w: 900, h: 460 }]]) {
        ids[id] = await P.evaluate(async ([id, g]) => { const h = window.__lab.frame.shellHandle(); const o = h.openWindow(id, { geometry: g }); await h.win(o).host.ready; return o; }, [id, g]);
      }
      await P.waitForTimeout(500);
      return ids;
    };
    const ids = await open6(false);
    const W = (id) => `.shell-window[data-window-app="${id}"]`;
    // Fenster nach vorn holen: Taskleisten-Klick nur, wenn es nicht schon vorn ist (sonst minimiert er).
    const front = async (id) => {
      const st = await P.evaluate((o) => { const r = window.__lab.frame.shellHandle().win(o); return { id: r.id, f: r.handle.element.classList.contains('is-focused') && r.handle.element.style.display !== 'none' }; }, ids[id]);
      if (!st.f) { await P.click(`[data-v4-win="${st.id}"]`); await P.waitForTimeout(120); }
    };

    // ---- Icons: eine Quelle (Launcher, Taskleiste, Fenster), Ersatz-Icon für Apps ohne Icon, kein Monogramm ----
    await P.waitForFunction(() => ['notes', 'crm', 'studio'].every((id) => {
      const w = document.querySelector(`.shell-window[data-window-app="${id}"] [data-window-app-icon]`);
      return w && w.complete && w.naturalWidth > 0 && w.style.visibility !== 'hidden';
    }), null, { timeout: 8000 }).catch(() => {});
    const icons = await P.evaluate(() => Object.fromEntries(['notes', 'crm', 'studio'].map((id) => {
      const launcher = document.querySelector(`[data-v4-app="${id}"] img`)?.getAttribute('src') || '';
      const win = document.querySelector(`.shell-window[data-window-app="${id}"]`);
      const wimg = win?.querySelector('[data-window-app-icon]')?.getAttribute('src') || '';
      const bar = [...document.querySelectorAll('[data-v4-win] img')].map((i) => i.getAttribute('src')).filter((s) => s === launcher).length;
      return [id, { same: !!launcher && launcher === wimg && bar >= 1, gen: launcher.startsWith('data:image/svg+xml'), src: document.querySelector(`[data-v4-app="${id}"] [data-v4-icon]`)?.dataset.v4IconSource,
        palette: win?.dataset.shellV2PaletteSource || '', monogram: !!win?.querySelector('.shell-window-v2-icon.is-fallback') || !!document.querySelector(`[data-v4-app="${id}"] .is-glyph`) }];
    })));
    check('FR6a Icon aus EINER Quelle: Launcher, Taskleiste und Fenster zeigen dieselbe URL; App ohne Icon bekommt ein Ersatz-Icon (Verlauf + Buchstabe, generated), nie ein Monogramm; Palette-Quelle nie leer',
      Object.values(icons).every((i) => i.same && !i.monogram) && icons.studio.gen && icons.studio.src === 'generated' && icons.studio.palette === 'generated'
        && icons.notes.src === 'icon' && icons.notes.palette === 'icon' && icons.crm.palette === 'icon', JSON.stringify(icons));

    // ---- G9 nahtlos: Kantenfarbe direkt neben dem Logo = Logo-Randfarbe (ΔE < 12), Pixelmessung im Screenshot ----
    const pixelAt = async (png, pts) => P.evaluate(async ([b64, pts]) => {
      const img = new Image(); img.src = `data:image/png;base64,${b64}`; await img.decode();
      const c = document.createElement('canvas'); c.width = img.naturalWidth; c.height = img.naturalHeight;
      const g = c.getContext('2d'); g.drawImage(img, 0, 0);
      return pts.map(([x, y]) => [...g.getImageData(Math.round(x), Math.round(y), 1, 1).data].slice(0, 3));
    }, [png.toString('base64'), pts]);
    const lab = ([r, g, b]) => {
      const f = (v) => { v /= 255; return v > 0.04045 ? ((v + 0.055) / 1.055) ** 2.4 : v / 12.92; };
      const [R, G, B] = [f(r), f(g), f(b)];
      const X = (R * 0.4124 + G * 0.3576 + B * 0.1805) / 0.95047; const Y = R * 0.2126 + G * 0.7152 + B * 0.0722; const Z = (R * 0.0193 + G * 0.1192 + B * 0.9505) / 1.08883;
      const h = (t) => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116);
      return [116 * h(Y) - 16, 500 * (h(X) - h(Y)), 200 * (h(Y) - h(Z))];
    };
    const dE = (a, b) => { const [p, q] = [lab(a), lab(b)]; return Math.hypot(p[0] - q[0], p[1] - q[1], p[2] - q[2]); };
    // PAL (M-17): gemessen am deckenden Logopixel, das der Ecke am nächsten liegt (v4/tools/geometry/g9.mjs) – das
    // Eckpixel zeigte bei transparenten runden Ecken den dunklen Halter und wertete eine dunkle Kante als „nahtlos“.
    const { g9Pixel } = await import('../../tools/geometry/g9.mjs');
    const g9 = async (appIds) => {
      const rows = [];
      for (const id of appIds) {
        const r = await g9Pixel(P, `.shell-window[data-window-app="${id}"]`, { label: id });
        rows.push(r.row ? { app: id, top: +r.row.top.toFixed(1), left: +r.row.left.toFixed(1) } : { app: id, top: 99, left: 99, skip: r.skip });
      }
      return rows;
    };
    // Fenster nacheinander nach vorn, damit jedes Logo und seine Kanten unverdeckt sind.
    const g9rows = [];
    for (const id of ['notes', 'crm', 'studio']) {
      await front(id);
      g9rows.push(...await g9([id]));
    }
    const studioJoint = await P.$eval(W('studio'), (w) => w.style.getPropertyValue('--shell-v2-frame-top-joint'));
    const studioLeft = await P.$eval(W('studio'), (w) => w.style.getPropertyValue('--shell-v2-frame-left-joint'));
    check('G9 nahtlos (Pixel): ΔE Logo-Rand ↔ Kante oben/links < 12 für alle Lab-Apps; falsches frame_palette im Manifest (Grün) wirkt nicht',
      g9rows.length === 3 && g9rows.every((r) => r.top < 12 && r.left < 12) && studioJoint && studioJoint.toLowerCase() !== '#00ff00', JSON.stringify({ g9rows, studioJoint }));
    await P.screenshot({ path: shot('fr6a-frame-1280.png') });
    // Mutationsprobe G9: eine händisch gesetzte (falsche) Kantenfarbe muss die Messung reißen.
    await P.$eval(W('studio'), (w) => { w.style.setProperty('--shell-v2-frame-top-joint', '#00ff00'); w.style.setProperty('--shell-v2-frame-left-joint', '#00ff00'); });
    await P.waitForTimeout(60);
    const g9mut = await g9(['studio']);
    check('G9 Mutationsprobe: händisch gesetzte Kantenfarbe wird erkannt (ΔE ≥ 12)', g9mut[0].top >= 12 && g9mut[0].left >= 12, JSON.stringify(g9mut));
    await P.$eval(W('studio'), (w, [j, l]) => { w.style.setProperty('--shell-v2-frame-top-joint', j); w.style.setProperty('--shell-v2-frame-left-joint', l); }, [studioJoint, studioLeft]);

    // ---- G0 Shell-Treue: Chrome des v4-Fensters gegen den app.js-Pfad (app.css) derselben App, pixelgenau ----
    // Referenz: dasselbe Fenster-DOM (Klon ohne App-Inhalt, gleiche Palette-Variablen) nur mit app.css + base.css gerendert.
    {
      const sel = W('studio');
      await front('studio');
      await P.mouse.move(2, 790);
      await P.waitForTimeout(200);
      const cap = await P.$eval(sel, (w) => {
        const r = w.getBoundingClientRect();
        const clone = w.cloneNode(true);
        clone.querySelector('[data-window-content]').replaceChildren();
        clone.querySelectorAll('[data-v4-zone-slot], .v4-status-zone__list, [data-v4-src-host]').forEach((n) => n.remove());
        // TOPBAR: Fensterebene liegt unter der Kopfleiste; Referenz = echte Shell-V2-Kopfleiste (app.css-Pfad) + Ebene am selben Ursprung.
        const lr = document.querySelector('[data-v4-layer]').getBoundingClientRect();
        const top = document.querySelector('[data-v4-topbar] > .app-shell');
        return { html: clone.outerHTML, r: { l: r.left, t: r.top, w: r.width, h: r.height }, layer: { l: lr.left, t: lr.top }, top: top ? top.innerHTML : '', bg: getComputedStyle(document.querySelector('[data-v4-frame]')).backgroundColor, theme: document.documentElement.dataset.theme || 'light' };
      });
      // Nur das Fenster-Chrome vergleichen: App-Inhalt und Launcher (Fläche hinter den Eckgriffen) ausblenden.
      const hideRest = (on) => P.$eval(sel, (w, on) => {
        const c = w.querySelector('[data-window-content]'); c.style.visibility = on ? 'hidden' : '';
        for (const l of document.querySelectorAll('[data-v4-launcher], [data-v4-desktop], .v4f-crew')) l.style.visibility = on ? 'hidden' : ''; // FR6d: Desktop statt Launcher-Fläche
        for (const o of document.querySelectorAll('.shell-window')) if (o !== w) o.style.visibility = on ? 'hidden' : '';
      }, on);
      await hideRest(true);
      const v4png = await P.screenshot();
      await hideRest(false);
      const R = await ctx6.newPage();
      await R.setViewportSize({ width: 1280, height: 800 });
      await R.goto(`${base}/v4/frame/lab/index.html?g0=1`).catch(() => {});
      await R.setContent(`<!doctype html><html lang="de" data-theme="${cap.theme}"><head><meta charset="utf-8"><link rel="stylesheet" href="${base}/app.css"><link rel="stylesheet" href="${base}/shared/base.css"><link rel="stylesheet" href="${base}/shared/shell-topbar.css">
        <style>html,body{margin:0;height:100%;background:${cap.bg} !important} .g0{position:fixed;left:${cap.layer.l}px;top:${cap.layer.t}px;right:0;bottom:0} .g0 .shell-window-content{visibility:hidden} .app-shell{background:transparent !important}</style></head>
        <body><div class="app-shell">${cap.top}</div><div class="g0">${cap.html}</div></body></html>`, { waitUntil: 'load' });
      await R.waitForTimeout(400);
      const refPng = await R.screenshot();
      // Bereiche: Icon-Header, vier Eckerweiterungen, Hover-Icons (Cluster); Kanten zählen nicht (G9 regelt die Farbe).
      const { l, t, w, h } = cap.r;
      // Rahmen 6 px: Polsterkante = Außenkante + 6; Icon bei -6 (bündig außen), Eckgriffe 32 px außen an der Polsterkante.
      const regions = { icon: [l, t, 80, 80], nw: [l - 26, t - 26, 32, 32], ne: [l + w - 6, t - 26, 32, 32], sw: [l - 26, t + h - 6, 32, 32], se: [l + w - 6, t + h - 6, 32, 32], cluster: [l + w - 100, t + 6, 94, 44] };
      const cmp = await P.evaluate(async ([a, b, regions]) => {
        const load = async (b64) => { const i = new Image(); i.src = `data:image/png;base64,${b64}`; await i.decode(); const c = document.createElement('canvas'); c.width = i.naturalWidth; c.height = i.naturalHeight; const g = c.getContext('2d'); g.drawImage(i, 0, 0); return g; };
        const [ga, gb] = [await load(a), await load(b)];
        const out = {};
        for (const [k, [x, y, w, h]] of Object.entries(regions)) {
          const da = ga.getImageData(Math.round(x), Math.round(y), w, h).data; const db = gb.getImageData(Math.round(x), Math.round(y), w, h).data;
          let same = 0; const n = da.length / 4;
          for (let i = 0; i < da.length; i += 4) if (Math.abs(da[i] - db[i]) + Math.abs(da[i + 1] - db[i + 1]) + Math.abs(da[i + 2] - db[i + 2]) <= 24) same++;
          out[k] = +(same / n).toFixed(4);
        }
        return out;
      }, [v4png.toString('base64'), refPng.toString('base64'), regions]);
      fs.writeFileSync(shot('fr6a-g0-v4-1280.png'), v4png);
      fs.writeFileSync(shot('fr6a-g0-ref-appcss-1280.png'), refPng);
      check('G0 Shell-Treue: Icon-Header, Eckerweiterungen und Hover-Icons des v4-Rahmens = app.js-Pfad (app.css) derselben App (≥ 98 % Pixel je Bereich)',
        Object.values(cmp).every((v) => v >= 0.98), JSON.stringify(cmp));
      // Mutationsprobe G0: ein verändertes Chrome (Icon-Header kleiner) muss auffallen.
      await P.$eval(sel, (w) => w.style.setProperty('--shell-v2-icon-size', '40px'));
      await hideRest(true);
      const mutPng = await P.screenshot();
      await hideRest(false);
      await P.$eval(sel, (w) => w.style.removeProperty('--shell-v2-icon-size'));
      const cmpMut = await P.evaluate(async ([a, b, r]) => {
        const load = async (b64) => { const i = new Image(); i.src = `data:image/png;base64,${b64}`; await i.decode(); const c = document.createElement('canvas'); c.width = i.naturalWidth; c.height = i.naturalHeight; const g = c.getContext('2d'); g.drawImage(i, 0, 0); return g; };
        const [ga, gb] = [await load(a), await load(b)];
        const da = ga.getImageData(Math.round(r[0]), Math.round(r[1]), r[2], r[3]).data; const db = gb.getImageData(Math.round(r[0]), Math.round(r[1]), r[2], r[3]).data;
        let same = 0; for (let i = 0; i < da.length; i += 4) if (Math.abs(da[i] - db[i]) + Math.abs(da[i + 1] - db[i + 1]) + Math.abs(da[i + 2] - db[i + 2]) <= 24) same++;
        return +(same / (da.length / 4)).toFixed(4);
      }, [mutPng.toString('base64'), refPng.toString('base64'), regions.icon]);
      check('G0 Mutationsprobe: verkleinerter Icon-Header weicht ab (< 98 %)', cmpMut < 0.98, String(cmpMut));
      await R.close();
    }

    // ---- Reservierte Zonen (G8-Regel im Rahmen-Labor): Werkzeugleiste oben rechts weicht aus, zweite Kopfzeile bleibt voll ----
    const zoneProbe = () => P.$eval(W('studio'), (w) => {
      const c = w.querySelector('.shell-window-content').getBoundingClientRect(); const cs = getComputedStyle(w.querySelector('.shell-window-content'));
      const ih = parseFloat(cs.getPropertyValue('--shell-icon-zone-height')); const ch = parseFloat(cs.getPropertyValue('--shell-controls-zone-height'));
      const iw = parseFloat(cs.getPropertyValue('--shell-icon-inset')); const cw = parseFloat(cs.getPropertyValue('--shell-controls-inset'));
      const zones = [[c.left, c.top, c.left + iw, c.top + ih], [c.right - cw, c.top, c.right, c.top + ch]];
      const ctl = w.querySelector('[data-window-controls]').getBoundingClientRect(); const ic = w.querySelector('.shell-window-v2-icon').getBoundingClientRect();
      const inter = (r, z) => r.width > 0 && r.height > 0 && r.left < z[2] && r.right > z[0] && r.top < z[3] && r.bottom > z[1];
      const els = [...w.querySelectorAll('.v4-app-root button, .v4-app-root a, .v4-app-root input, .v4-app-root select, .v4-app-root [role=button], .v4-app-root [tabindex]')];
      const hits = els.filter((e) => zones.some((z) => inter(e.getBoundingClientRect(), z))).map((e) => e.outerHTML.slice(0, 40));
      const head = w.querySelector('[data-studio-head]').getBoundingClientRect(); const search = w.querySelector('[data-studio-search]').getBoundingClientRect(); const filter = w.querySelector('[data-studio-filter]').getBoundingClientRect();
      // Zeile 2 (unter dem Icon, unterhalb des Clusters): links eingerückt, rechts volle Breite; Zeile 3 (unter beiden Zonen): voll.
      return { hits, zones, zonesCoverChrome: ctl.left >= zones[1][0] - 0.5 && ctl.bottom <= zones[1][3] + 0.5 && ic.right <= zones[0][2] + 0.5 && ic.bottom <= zones[0][3] + 0.5,
        searchFull: Math.abs(search.right - (head.right - 6)) <= 1 && search.left >= zones[0][2] - 0.5 && Math.abs(filter.left - (head.left + 6)) <= 1 && Math.abs(filter.right - (head.right - 6)) <= 1,
        search: [Math.round(search.left - head.left), Math.round(head.right - search.right)], filter: [Math.round(filter.left - head.left), Math.round(head.right - filter.right)] };
    });
    const zk = await zoneProbe();
    await P.screenshot({ path: shot('fr6a-zones-1280.png') });
    check('G8 Rahmen-Labor: kein Bedienelement der App schneidet die reservierten Zonen (Icon oben links, Knopfcluster oben rechts); Zonen decken das schwebende Chrome; eingerückt wird nur, was eine Zone schneidet (Zeile 2 nur links, Zeile 3 voll)',
      zk.hits.length === 0 && zk.zonesCoverChrome && zk.searchFull, JSON.stringify(zk));

    // ---- Andock-Raster: Hover zeigt 3×3 + Freie Größe + Minimieren, räumlich, deutsch, Tastatur, aktuelle Lage ----
    await front('studio');
    await P.hover(`${W('studio')} [data-window-layout-trigger]`);
    await P.waitForSelector(`${W('studio')} [data-window-layout-menu]:not([hidden])`, { timeout: 3000 }).catch(() => {});
    const grid = await P.$eval(W('studio'), (w) => {
      const m = w.querySelector('[data-window-layout-menu]'); const o = [...m.querySelectorAll('[data-window-layout-control]')];
      const r = Object.fromEntries(o.map((b) => [b.dataset.windowLayoutControl, b.getBoundingClientRect()]));
      const cx = (k) => r[k].left + r[k].width / 2; const cy = (k) => r[k].top + r[k].height / 2;
      return { open: !m.hidden, order: o.map((b) => b.dataset.windowLayoutControl), labels: o.map((b) => b.getAttribute('aria-label')),
        spatial: cx('left') < cx('maximize') && cx('maximize') < cx('right') && cy('top') < cy('maximize') && cy('maximize') < cy('bottom')
          && cx('top-left') < cx('top-right') && cy('top-left') < cy('bottom-left') && cx('bottom-right') > cx('bottom-left') && cy('bottom-right') > cy('top-right')
          && cy('free') > cy('bottom') && cy('minimize') > cy('free') };
    });
    await P.screenshot({ path: shot('fr6a-grid-1280.png') });
    check('FR6a Andock-Raster bei Hover: 3×3 räumlich (Ecken, Kanten, Mitte = Maximieren), darunter „Freie Größe“, „Minimieren“ und (M-11) „Vollbild“, deutsche Bezeichnungen',
      grid.open && grid.spatial && grid.order.join(',') === 'top-left,top,top-right,left,maximize,right,bottom-left,bottom,bottom-right,free,minimize,fullscreen'
        && grid.labels.includes('Linke Hälfte') && grid.labels.includes('Unten rechts anordnen') && grid.labels.includes('Freie Größe'), JSON.stringify(grid));
    await P.click(`${W('studio')} [data-window-layout-control="left"]`);
    const afterLeft = await P.$eval(W('studio'), (w) => w.dataset.snapZone || '');
    await P.focus(`${W('studio')} [data-window-layout-trigger]`);
    await P.keyboard.press('Enter');
    const cur = await P.$eval(W('studio'), (w) => ({ current: w.querySelector('[data-window-layout-menu] [aria-current="true"]')?.dataset.windowLayoutControl, focus: document.activeElement?.dataset?.windowLayoutControl }));
    await P.keyboard.press('ArrowRight');
    await P.keyboard.press('ArrowRight');
    await P.keyboard.press('ArrowDown');
    const focusDown = await P.evaluate(() => document.activeElement?.dataset?.windowLayoutControl);
    await P.keyboard.press('Enter');
    const afterKey = await P.$eval(W('studio'), (w) => ({ zone: w.dataset.snapZone || '', open: !w.querySelector('[data-window-layout-menu]').hidden }));
    await P.focus(`${W('studio')} [data-window-layout-trigger]`);
    await P.keyboard.press('Enter');
    await P.keyboard.press('Escape');
    const escClosed = await P.$eval(W('studio'), (w) => w.querySelector('[data-window-layout-menu]').hidden);
    await P.keyboard.press('Control+Alt+KeyU');
    const viaKeyU = await P.$eval(W('studio'), (w) => w.dataset.snapZone || '');
    await P.keyboard.press('Control+Alt+ArrowRight');
    const viaKeyR = await P.$eval(W('studio'), (w) => w.dataset.snapZone || '');
    check('FR6a Raster per Klick und Tastatur: aktuelle Lage markiert und fokussiert, Pfeile bewegen räumlich (links → Mitte → rechts → unten rechts), Enter wendet an, Esc schließt; Strg+Wahl+U / Strg+Wahl+→',
      afterLeft === 'left' && cur.current === 'left' && cur.focus === 'left' && focusDown === 'bottom-right' && afterKey.zone === 'bottom-right' && !afterKey.open && escClosed && viaKeyU === 'top-left' && viaKeyR === 'right',
      JSON.stringify({ afterLeft, cur, focusDown, afterKey, escClosed, viaKeyU, viaKeyR }));
    await P.click(`${W('studio')} [data-window-layout-trigger]`);
    await P.click(`${W('studio')} [data-window-layout-control="free"]`);

    // ---- Doppelklick auf freie Stelle des Pane-Kopfs maximiert / stellt wieder her ----
    await P.dblclick(`${W('studio')} [data-studio-title]`);
    const dbl1 = await P.$eval(W('studio'), (w) => w.classList.contains('is-maximized'));
    await P.dblclick(`${W('studio')} [data-studio-title]`);
    const dbl2 = await P.$eval(W('studio'), (w) => w.classList.contains('is-maximized'));
    check('FR6a Doppelklick auf freie Stelle des Pane-Kopfs: maximieren, erneut: wiederherstellen', dbl1 && !dbl2, JSON.stringify({ dbl1, dbl2 }));

    // ---- Vollbild über die Shell (Vollbild-Knopf der App ruft api.shell.window.fullscreen) ----
    await P.click(`${W('studio')} [data-studio-full]`);
    await P.waitForTimeout(250);
    const fs1 = await P.evaluate(() => ({ el: !!document.fullscreenElement && !!document.fullscreenElement.closest('.shell-window[data-window-app="studio"]'), ret: globalThis.__studioFull }));
    await P.screenshot({ path: shot('fr6a-fullscreen-1280.png') });
    const fs2 = await P.evaluate(async (o) => { const api = window.__lab.frame.shellHandle().win(o).host.instance.api; return { exit: await api.shell.window.exitFullscreen(), el: !!document.fullscreenElement }; }, ids.studio);
    await P.click(`${W('studio')} [data-studio-title]`);
    await P.keyboard.press('F11');
    await P.waitForTimeout(250);
    const fs3 = await P.evaluate(() => !!document.fullscreenElement?.closest('.shell-window[data-window-app="studio"]'));
    await P.keyboard.press('F11');
    await P.waitForTimeout(250);
    const fs4 = await P.evaluate(() => !!document.fullscreenElement);
    check('FR6a Vollbild: api.shell.window.fullscreen() (App-Knopf) → Fensterinhalt im Element-Vollbild; exitFullscreen() beendet; F11 schaltet für das fokussierte Fenster',
      fs1.el && fs1.ret === true && fs2.exit === true && !fs2.el && fs3 && !fs4, JSON.stringify({ fs1, fs2, fs3, fs4 }));

    // ---- 420 px: Zonen gelten, zweite Kopfzeile bleibt voll breit ----
    await P.setViewportSize({ width: 420, height: 800 });
    await front('studio');
    await P.waitForTimeout(400);
    const z420 = await zoneProbe();
    await P.screenshot({ path: shot('fr6a-zones-420.png') });
    check('G8 bei 420 px: keine App-Bedienelemente in den Zonen; nur Zonen-Schnitt eingerückt (Suchfeld rechts voll, Filterzeile ganz voll)', z420.hits.length === 0 && z420.searchFull, JSON.stringify(z420));
    await P.setViewportSize({ width: 1280, height: 800 });

    // ---- Mutationsprobe G8: dieselbe Leiste OHNE Kit schneidet die Zonen (die Prüfung findet es) ----
    // ZONE2: ohne Kit greift die allgemeine Zonen-Garantie des Rahmens (App-Wurzel beginnt unter der Zonenhöhe) → keine
    // Treffer. Mutationsprobe = Garantie per Stil neutralisiert → die Prüfung findet die Knöpfe unter Cluster bzw. Icon.
    await open6(true);
    const zg = await zoneProbe();
    const zgAttr = await P.$eval(W('studio'), (w) => w.querySelector('.v4-app-root')?.getAttribute('data-v4-zone-guard') ?? null);
    check('G8 ohne Kit-Kopf (ZONE2): die Zonen-Garantie rückt die ganze App aus, kein Bedienelement schneidet eine Zone', zg.hits.length === 0 && zgAttr === 'top', JSON.stringify({ hits: zg.hits, zgAttr }));
    await P.addStyleTag({ content: '.v4f .shell-window-content > .v4-app-root[data-v4-zone-guard] { top: 0 !important; right: 0 !important; }' });
    await P.waitForTimeout(80);
    const zm = await zoneProbe();
    check('G8 Mutationsprobe: ohne Kit-Kopf und ohne Zonen-Garantie liegen Knöpfe unter dem Cluster bzw. Icon – die Prüfung findet sie', zm.hits.length > 0, JSON.stringify(zm.hits));
    check('FR6a ohne Seitenfehler', perr.length === 0, perr.join(' | '));
    await ctx6.close();
  }

  // ---- FR6b (eigene Seite) ----
  await fr6b();
  // ---- FR6c (eigene Seite, Mutationsläufe je Punkt) ----
  await fr6c();
  // ---- Z45 (eigene Seite) ----
  await z45();
  // ---- TOPBAR (eigene Seite) ----
  await topbarLab();
  // ---- ZONE2 (eigene Seite) ----
  await zone2();
  // ---- MOBILE (WJB-M, eigene Seiten; nur mock) ----
  await mobile();
  // ---- ICON25 (eigene Seite) ----
  await icon25();
  // ---- M-09 (eigene Seite, nur Stub) ----
  await m09();
  // AGENT: nur gegen den Stub (die Rahmen-Attrappe hat kein kernel.source).
  if (STUB) await ag();
  // LOOP: nur gegen den Stub ab §16.44 + draft.write.result (ältere Pins: Block entfällt).
  if (STUB) await loop();
  // ---- FRX (eigene Seiten; M-02 nur stub) ----
  await frx();
} catch (error) {
  if (!error?.fr6bOnly) check('Lauf ohne Ausnahme', false, String(error?.stack || error));
} finally {
  await browser.close();
  server.close();
}
const failed = results.filter((r) => !r.ok);
fs.writeFileSync(path.join(here, 'shots', `results-${MODE}.json`), JSON.stringify(results, null, 2));
console.log(`\n${results.length - failed.length}/${results.length} bestanden`);
process.exit(failed.length ? 1 : 0);
