// Shell v4 – Messung App-Host / Live-Remount / Zustandszone (Playwright, headless, 2 Tabs im selben Kontext).
// Aufruf: node src/apps/business-os/v4/lab/app-host/measure.mjs [--reps 20]
// Schreibt nach JEDEM Szenario results.json und den automatisch erzeugten Messwert-Block in RESULTS.md.
import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import os from 'node:os';
import { startServer } from './server.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const PW = path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright/index.mjs');
const EXE = path.join(os.homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');
const { chromium } = await import(PW);
const REPS = Number(process.argv[process.argv.indexOf('--reps') + 1]) || 20;
const ONLY = process.argv.includes('--only') ? process.argv[process.argv.indexOf('--only') + 1].split(',') : null;

const results = { startedAt: new Date().toISOString(), env: { node: process.version, platform: `${os.platform()} ${os.arch()}`, cpus: os.cpus().length, browser: null, reps: REPS }, scenarios: {} };
const pct = (xs, p) => { const s = [...xs].sort((a, b) => a - b); return s.length ? s[Math.min(s.length - 1, Math.ceil((p / 100) * s.length) - 1)] : null; };
const r1 = (x) => (x == null ? null : Math.round(x * 10) / 10);
const stats = (xs) => ({ n: xs.length, p50: r1(pct(xs, 50)), p95: r1(pct(xs, 95)), min: r1(Math.min(...xs)), max: r1(Math.max(...xs)) });

async function flush() {
  await writeFile(path.join(HERE, 'results.json'), JSON.stringify(results, null, 2));
  const md = path.join(HERE, 'RESULTS.md');
  let text = '';
  try { text = await readFile(md, 'utf8'); } catch { text = '# Shell v4 · App-Host – Ergebnisse\n\n<!-- MESSWERTE:START -->\n<!-- MESSWERTE:END -->\n'; }
  const lines = [`<!-- MESSWERTE:START -->`, `_Automatisch von measure.mjs, Lauf ${results.startedAt}, ${results.env.browser || ''}, ${results.env.platform}, ${results.env.cpus} CPUs, Node ${results.env.node}._`, ''];
  for (const [name, s] of Object.entries(results.scenarios)) {
    lines.push(`### ${name} — ${s.pass ? 'BESTANDEN' : 'NICHT BESTANDEN'}`, '', s.summary || '', '', '```json', JSON.stringify(s.data, null, 1).slice(0, 4000), '```', '');
  }
  lines.push('<!-- MESSWERTE:END -->');
  text = text.replace(/<!-- MESSWERTE:START -->[\s\S]*<!-- MESSWERTE:END -->/, lines.join('\n'));
  await writeFile(md, text);
}
async function scenario(name, fn) {
  if (ONLY && !ONLY.some((o) => name.startsWith(o))) return;
  const t = Date.now();
  try {
    const out = await fn();
    results.scenarios[name] = { ...out, durationS: (Date.now() - t) / 1000 };
  } catch (err) {
    results.scenarios[name] = { pass: false, summary: `Abbruch: ${err.message}`, data: { stack: String(err.stack).split('\n').slice(0, 6) } };
  }
  console.log(`[${name}] ${results.scenarios[name].pass ? 'PASS' : 'FAIL'} ${results.scenarios[name].summary}`);
  await flush();
}

// ---------------------------------------------------------------------------------------------------------------
const { server, url } = await startServer();
const browser = await chromium.launch({ executablePath: EXE, headless: true });
results.env.browser = `chromium-headless-shell ${browser.version()}`;
const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
const requests = [];
ctx.on('request', (r) => requests.push({ url: r.url().replace(url, ''), method: r.method(), type: r.resourceType() }));
const consoleErrors = [];
const navs = { A: 0, B: 0 };
async function openTab(name) {
  const p = await ctx.newPage();
  p.on('console', (m) => { if (m.type() === 'error') consoleErrors.push(`${name}: ${m.text()}`); });
  p.on('pageerror', (e) => consoleErrors.push(`${name}: pageerror ${e.message}`));
  p.on('framenavigated', (f) => { if (f === p.mainFrame()) navs[name]++; });
  await p.goto(`${url}/?tab=${name}`);
  await p.waitForFunction(() => document.documentElement.dataset.labReady === '1');
  return p;
}
const A = await openTab('A');
await A.waitForFunction(() => window.__lab.isLeader());
const B = await openTab('B');
const sentinels = { A: await A.evaluate(() => window.__lab.sentinel), B: await B.evaluate(() => window.__lab.sentinel) };

const waitState = (page, { windowId, ver, state, t0 }, timeout = 20000) => page.waitForFunction(
  ({ windowId, ver, state, t0 }) => window.__lab.events.find((e) => e.windowId === windowId && e.ver === ver && e.state === state && (t0 == null || e.t0 === t0)),
  { windowId, ver, state, t0 }, { timeout, polling: 5 },
).then((h) => h.jsonValue());

// Zustand eines Fensters im Formular aus Sicht des Nutzers lesen (Werte, Fokus, Auswahl, Scroll).
const readForm = (page, windowId) => page.evaluate((id) => {
  const w = window.__lab.win(id);
  const root = w.el.querySelector('.v4-app-root');
  const q = (s) => root.querySelector(s);
  const a = document.activeElement;
  const fields = {};
  for (const el of root.querySelectorAll('input, textarea, select')) {
    const k = el.id || el.getAttribute('name');
    fields[k] = el.type === 'checkbox' ? el.checked : el.value;
  }
  const scroll = {};
  for (const el of root.querySelectorAll('[id]')) if (el.scrollHeight > el.clientHeight) scroll[el.id] = el.scrollTop;
  return {
    ver: w.ver, state: w.state, fields,
    focus: a && root.contains(a) ? (a.id || a.getAttribute('name')) : null,
    selection: a && root.contains(a) && typeof a.selectionStart === 'number' ? [a.selectionStart, a.selectionEnd] : null,
    scroll, hasFocus: document.hasFocus(),
  };
}, windowId);

let wA; let wB; let wN; let w3;
await scenario('a-remount-zwei-tabs', async () => {
  wA = await A.evaluate(() => window.__lab.openWindow('formular', { geometry: { x: 20, y: 20, w: 560, h: 460 } }));
  wB = await B.evaluate(() => window.__lab.openWindow('formular', { geometry: { x: 20, y: 20, w: 560, h: 460 } }));
  await waitState(B, { windowId: wB, ver: '1', state: 'mounted' });
  // Tab B: echte Eingaben
  const sel = (s) => `[data-window-id="${wB}"] ${s}`;
  await B.click(sel('#name'));
  await B.keyboard.type('Erika Mustermann');
  await B.selectOption(sel('#kategorie'), 'c');
  await B.click(sel('#beschreibung'));
  await B.keyboard.type(Array.from({ length: 25 }, (_, i) => `Zeile ${i} der Beschreibung`).join('\n'));
  await B.evaluate((id) => {
    const root = window.__lab.win(id).el;
    const ta = root.querySelector('#beschreibung');
    ta.setSelectionRange(40, 52);
    ta.scrollTop = 120;
    root.querySelector('#liste').scrollTop = 200;
  }, wB);
  const times = { A: [], B: [] };
  const phasesB = [];
  const mismatches = [];
  for (let i = 0; i < REPS; i++) {
    const ver = i % 2 === 0 ? '2' : '1';
    // Weitertippen an der Einfügemarke, dann Erwartung festhalten
    await B.keyboard.type(`·${i}`);
    await B.evaluate((id) => { const ta = window.__lab.win(id).el.querySelector('#beschreibung'); ta.scrollTop = 120; }, wB);
    const before = await readForm(B, wB);
    // Gegenprobe echter Remount: alter root bekommt eine Marke, nach dem Swap muss ein neuer root da sein.
    await B.evaluate(({ id, i }) => { window.__lab.win(id).el.querySelector('.v4-app-root').dataset.labMarker = String(i); }, { id: wB, i });
    const t0 = await A.evaluate((v) => window.__lab.publish('formular', v), ver);
    const [ea, eb] = await Promise.all([waitState(A, { windowId: wA, ver, state: 'mounted', t0 }), waitState(B, { windowId: wB, ver, state: 'mounted', t0 })]);
    await B.waitForTimeout(30); // zweiter Restore-Durchgang (setTimeout 0)
    times.A.push(ea.at - t0);
    times.B.push(eb.at - t0);
    phasesB.push({ toSwapStart: null, ...Object.fromEntries(Object.entries(eb.phases || {}).map(([k, v]) => [k, r1(v)])) });
    const after = await readForm(B, wB);
    const diffs = [];
    for (const k of ['name', 'beschreibung', 'kategorie']) if (before.fields[k] !== after.fields[k]) diffs.push(`${k}: ${JSON.stringify(before.fields[k]).slice(0, 40)} → ${JSON.stringify(after.fields[k]).slice(0, 40)}`);
    if (before.focus !== after.focus) diffs.push(`focus ${before.focus} → ${after.focus}`);
    if (JSON.stringify(before.selection) !== JSON.stringify(after.selection)) diffs.push(`selection ${before.selection} → ${after.selection}`);
    for (const k of ['beschreibung', 'liste']) if (before.scroll[k] !== after.scroll[k]) diffs.push(`scroll ${k} ${before.scroll[k]} → ${after.scroll[k]}`);
    if (after.ver !== ver) diffs.push(`ver ${after.ver}`);
    const stale = await B.evaluate((id) => window.__lab.win(id).el.querySelectorAll('.v4-app-root').length !== 1 || !!window.__lab.win(id).el.querySelector('.v4-app-root[data-lab-marker]'), wB);
    if (stale) diffs.push('kein neuer root (kein echter Remount)');
    if (diffs.length) mismatches.push({ rep: i, from: before.ver, to: ver, diffs });
  }
  const last = await readForm(B, wB);
  await B.screenshot({ path: path.join(HERE, 'shots/a-tab-b-nach-20-remounts.png') });
  const phaseStats = Object.fromEntries(['snapshot', 'persist', 'load', 'unmount', 'mount'].map((k) => [k, stats(phasesB.map((p) => p[k]).filter((x) => x != null))]));
  const data = { publishToMountedMs: { tabA_leader: stats(times.A), tabB_reader: stats(times.B) }, rawB: times.B.map(r1), phasesB_first2: phasesB.slice(0, 2), phaseStatsB: phaseStats, mismatches, finalState: { ver: last.ver, focus: last.focus, selection: last.selection, scroll: last.scroll, nameValue: last.fields.name, beschreibungLen: last.fields.beschreibung.length, prio: last.fields.prio ?? null } };
  return { pass: mismatches.length === 0, summary: `${REPS} Remounts v1↔v2 in beiden Tabs; Veröffentlichen→mounted B p50 ${data.publishToMountedMs.tabB_reader.p50} ms / p95 ${data.publishToMountedMs.tabB_reader.p95} ms, A p50 ${data.publishToMountedMs.tabA_leader.p50} / p95 ${data.publishToMountedMs.tabA_leader.p95} ms; Abweichungen Wert/Fokus/Auswahl/Scroll: ${mismatches.length}.`, data };
});

await scenario('b-generische-sicherung', async () => {
  wN = await B.evaluate(() => window.__lab.openWindow('notizen', { geometry: { x: 600, y: 20, w: 520, h: 420 } }));
  await waitState(B, { windowId: wN, ver: '1', state: 'mounted' });
  const sel = (s) => `[data-window-id="${wN}"] ${s}`;
  await B.click(sel('#notiz'));
  await B.keyboard.type(Array.from({ length: 12 }, (_, i) => `Notiz ${i}`).join('\n'));
  await B.selectOption(sel('select[name="farbe"]'), 'blau');
  await B.check(sel('input[name="wichtig"]'));
  await B.click(sel('#titel'));
  await B.keyboard.type('Einkauf Samstag');
  await B.evaluate((id) => {
    const root = window.__lab.win(id).el;
    root.querySelector('#titel').setSelectionRange(2, 7);
    root.querySelector('#verlauf').scrollTop = 300;
    root.querySelector('#notiz').scrollTop = 30;
  }, wN);
  const before = await readForm(B, wN);
  const t0 = await A.evaluate(() => window.__lab.publish('notizen', '2'));
  const ev = await waitState(B, { windowId: wN, ver: '2', state: 'mounted', t0 });
  await B.waitForTimeout(30);
  const after = await readForm(B, wN);
  const snap = await B.evaluate((id) => window.__lab.kernel.windowState.get(`B/mount:${id}`), wN);
  const diffs = [];
  for (const k of Object.keys(before.fields)) if (before.fields[k] !== after.fields[k]) diffs.push(`${k}: ${before.fields[k]} → ${after.fields[k]}`);
  if (before.focus !== after.focus) diffs.push(`focus ${before.focus} → ${after.focus}`);
  if (JSON.stringify(before.selection) !== JSON.stringify(after.selection)) diffs.push(`selection ${before.selection} → ${after.selection}`);
  for (const k of ['verlauf', 'notiz']) if (before.scroll[k] !== after.scroll[k]) diffs.push(`scroll ${k} ${before.scroll[k]} → ${after.scroll[k]}`);
  const data = { publishToMountedMs: r1(ev.at - t0), before, after, diffs, windowStateRecord: { kind: snap?.rec?.generic?.kind, fieldKeys: Object.keys(snap?.rec?.generic?.fields || {}), view: snap?.rec?.view, app: snap?.rec?.app } };
  return { pass: diffs.length === 0 && after.ver === '2', summary: `notizen v1→v2 ohne snapshot(): ${Object.keys(before.fields).length} Felder (input/select/checkbox/textarea), Fokus+Auswahl, 2 Scrollpositionen; Abweichungen: ${diffs.length}; ${data.publishToMountedMs} ms.`, data };
});

await scenario('c-rueckfall-v3', async () => {
  // Ausgangslage v2 sicherstellen
  const cur = (await readForm(B, wB)).ver;
  if (cur !== '2') { const t = await A.evaluate(() => window.__lab.publish('formular', '2')); await waitState(B, { windowId: wB, ver: '2', state: 'mounted', t0: t }); }
  await B.click(`[data-window-id="${wB}"] #beschreibung`);
  await B.keyboard.press('End');
  await B.keyboard.type(' – vor v3');
  const before = await readForm(B, wB);
  const t0 = await A.evaluate(() => window.__lab.publish('formular', '3'));
  const [ea, eb] = await Promise.all([
    waitState(A, { windowId: wA, ver: '2', state: 'rolled-back', t0 }, 20000),
    waitState(B, { windowId: wB, ver: '2', state: 'rolled-back', t0 }, 20000),
  ]);
  await B.waitForTimeout(30);
  const after = await readForm(B, wB);
  const zoneB = await B.evaluate((id) => window.__lab.winInfo(id).zone, wB);
  await B.screenshot({ path: path.join(HERE, 'shots/c-rueckfall-zone-tab-b.png') });
  const diffs = [];
  for (const k of ['name', 'beschreibung', 'kategorie', 'prio']) if (before.fields[k] !== after.fields[k]) diffs.push(k);
  if (before.focus !== after.focus) diffs.push('focus');
  await A.waitForTimeout(100);
  const pointer = await B.evaluate(() => { const c = window.__lab.kernel.activation.current('formular'); return { ver: String(c.ver), state: c.state, reverted: !!c.reverted }; });
  const readReports = (page) => page.evaluate(() => window.__lab.kernel.activation.reports.map((r) => ({ ver: r.ver, tab: r.tab })));
  const reports = [...await readReports(A), ...await readReports(B)];
  // Vorwärts zurückrollen: v2 erneut veröffentlichen → Fenster wieder mounted, Fehler weg.
  const t1 = await A.evaluate(() => window.__lab.publish('formular', '2'));
  await waitState(B, { windowId: wB, ver: '2', state: 'mounted', t0: t1 });
  const zoneAfterForward = await B.evaluate((id) => window.__lab.winInfo(id).zone, wB);
  const data = { publishToRolledBackMs: { A: r1(ea.at - t0), B: r1(eb.at - t0) }, ver: after.ver, zone: { hidden: zoneB.hidden, level: zoneB.level, text: zoneB.text, inChrome: zoneB.inChrome }, inputDiffs: diffs, pointerAfterRollback: pointer, reportFailed: reports, afterForwardRollback: { hidden: zoneAfterForward.hidden, entries: zoneAfterForward.entries } };
  const within10s = data.publishToRolledBackMs.A <= 10000 && data.publishToRolledBackMs.B <= 10000;
  const pass = after.ver === '2' && zoneB.level === 'error' && !zoneB.hidden && zoneB.inChrome && diffs.length === 0 && zoneAfterForward.hidden && within10s && pointer.ver === '2' && reports.some((r) => r.ver === '3');
  return { pass, summary: `v3 (mount hängt, Frist 9 s) → Rückfall auf v2 nach ${data.publishToRolledBackMs.B} ms (B) / ${data.publishToRolledBackMs.A} ms (A), ≤ 10 s: ${within10s}; reportFailed gemeldet (${reports.length}×), Zeiger danach ${pointer.ver}${pointer.reverted ? ' (zurückgenommen)' : ''}; Zone: „${zoneB.text}“; Eingabe-Abweichungen: ${diffs.length}; nach erneutem Veröffentlichen von v2 Zone leer: ${zoneAfterForward.hidden}.`, data };
});

await scenario('c2-rueckfall-mount-wirft', async () => {
  await B.click(`[data-window-id="${wB}"] #name`);
  await B.keyboard.press('End');
  await B.keyboard.type(' (vor v4)');
  const before = await readForm(B, wB);
  const t0 = await A.evaluate(() => window.__lab.publish('formular', '4'));
  const eb = await waitState(B, { windowId: wB, ver: '2', state: 'rolled-back', t0 }, 15000);
  await B.waitForTimeout(30);
  const after = await readForm(B, wB);
  const zoneB = await B.evaluate((id) => window.__lab.winInfo(id).zone, wB);
  const diffs = ['name', 'beschreibung', 'kategorie', 'prio'].filter((k) => before.fields[k] !== after.fields[k]);
  if (before.focus !== after.focus) diffs.push('focus');
  const t1 = await A.evaluate(() => window.__lab.publish('formular', '2'));
  await waitState(B, { windowId: wB, ver: '2', state: 'mounted', t0: t1 });
  const data = { publishToRolledBackMs: r1(eb.at - t0), zone: { level: zoneB.level, text: zoneB.text }, diffs };
  return { pass: after.ver === '2' && zoneB.level === 'error' && diffs.length === 0, summary: `v4 (mount wirft) → Rückfall auf v2 nach ${data.publishToRolledBackMs} ms; Zone: „${zoneB.text}“; Abweichungen ${diffs.length}.`, data };
});

await scenario('d-live-daten', async () => {
  const lat = [];
  const latLocal = [];
  for (let i = 0; i < REPS; i++) {
    const id = `live-${i}-${Date.now()}`;
    const waitB = B.evaluate(({ win, id }) => new Promise((res) => {
      const ul = window.__lab.win(win).el.querySelector('#liste');
      const mo = new MutationObserver(() => { if (ul.querySelector(`[data-id="${id}"]`)) { mo.disconnect(); res(window.__lab.now()); } });
      mo.observe(ul, { childList: true, subtree: true });
    }), { win: wB, id });
    await B.waitForTimeout(5);
    const [t0, tLocal] = await A.evaluate(({ win, id }) => new Promise((res) => {
      const ul = window.__lab.win(win).el.querySelector('#liste');
      const t0 = window.__lab.now();
      window.__lab.put({ id, titel: `Live ${id}`, prio: 9 });
      // lokale Anzeige im eigenen Fenster (synchroner Weg)
      res([t0, ul.querySelector(`[data-id="${id}"]`) ? window.__lab.now() : null]);
    }), { win: wA, id });
    const tB = await waitB;
    lat.push(tB - t0);
    if (tLocal) latLocal.push(tLocal - t0);
  }
  // Befehl: simulierte Folge queued→accepted→running→done, beobachtet in Tab B (nicht ausführender Tab)
  const cmd = await A.evaluate(() => window.__lab.command('lab.echo', { x: 1 }).id);
  const seen = await B.evaluate((id) => new Promise((res) => {
    const states = [];
    const live = window.__lab.kernel.appApi('formular', 'lab').get('business_commands', id);
    const t0 = window.__lab.now();
    const off = live.subscribe((d) => { if (d && states[states.length - 1] !== d.status) states.push(d.status); if (d?.status === 'done') { off(); res({ states, ms: window.__lab.now() - t0 }); } });
    setTimeout(() => res({ states, timeout: true }), 3000);
  }), cmd);
  const data = { putToOtherTabDomMs: stats(lat), putToOwnTabDomMs: latLocal.length ? stats(latLocal) : 'synchron (im selben Takt)', commandStatesSeenInB: seen };
  return { pass: lat.length === REPS && seen.states?.includes('done'), summary: `${REPS} Schreibvorgänge Tab A → Liste in Tab B sichtbar p50 ${data.putToOtherTabDomMs.p50} ms / p95 ${data.putToOtherTabDomMs.p95} ms (BroadcastChannel nach IDB-Commit); Befehl in B gesehen: ${(seen.states || []).join('→')}.`, data };
});

await scenario('e-shell-live-tausch', async () => {
  w3 = await B.evaluate(() => window.__lab.openWindow('formular', { geometry: { x: 300, y: 380, w: 480, h: 380 } }));
  await waitState(B, { windowId: w3, ver: '2', state: 'mounted' });
  // halbes Formular im dritten Fenster, Fokus dort
  await B.click(`[data-window-id="${w3}"] #name`);
  await B.keyboard.type('Halb ausgefüllt');
  const ids = [wB, wN, w3];
  const before = {};
  for (const id of ids) before[id] = { form: await readForm(B, id), info: await B.evaluate((i) => window.__lab.winInfo(i), id) };
  const swap = await B.evaluate(() => window.__lab.swapShell('2'));
  await B.waitForTimeout(30);
  await B.screenshot({ path: path.join(HERE, 'shots/e-nach-shell-tausch-tab-b.png') });
  const after = {};
  const diffs = [];
  for (const id of ids) {
    after[id] = { form: await readForm(B, id), info: await B.evaluate((i) => window.__lab.winInfo(i), id) };
    const b = before[id]; const a = after[id];
    if (JSON.stringify(b.form.fields) !== JSON.stringify(a.form.fields)) diffs.push(`${id} fields`);
    if (b.form.focus !== a.form.focus) diffs.push(`${id} focus ${b.form.focus}→${a.form.focus}`);
    if (JSON.stringify(b.form.selection) !== JSON.stringify(a.form.selection)) diffs.push(`${id} selection`);
    if (JSON.stringify(b.form.scroll) !== JSON.stringify(a.form.scroll)) diffs.push(`${id} scroll ${JSON.stringify(b.form.scroll)}→${JSON.stringify(a.form.scroll)}`);
    const gb = b.info.geometry; const ga = a.info.geometry;
    if (gb.x !== ga.x || gb.y !== ga.y || gb.w !== ga.w || gb.h !== ga.h) diffs.push(`${id} geometry`);
    if (b.form.ver !== a.form.ver) diffs.push(`${id} ver`);
  }
  const shellState = await B.evaluate(() => ({ ver: window.__lab.shellVer, desktops: document.querySelectorAll('[data-v4-frame]').length, desktopVer: document.querySelector('[data-v4-frame]').dataset.shellVer, shellCss: [...document.querySelectorAll('link[data-v4-frame-css]')].map((l) => l.getAttribute('href')), appCss: [...document.querySelectorAll('link[data-app-css]')].map((l) => l.getAttribute('href')) }));
  // Nach dem Tausch muss die neue Shell weiter remounten können (Abonnement auf den Aktivierungszeiger).
  const t0 = await A.evaluate(() => window.__lab.publish('formular', '1'));
  const ev = await waitState(B, { windowId: w3, ver: '1', state: 'mounted', t0 });
  const data = { swap: Object.fromEntries(Object.entries(swap).map(([k, v]) => [k, typeof v === 'number' ? r1(v) : v])), diffs, shellState, remountAfterSwapMs: r1(ev.at - t0), focusBefore: before[w3].form.focus, focusAfter: after[w3].form.focus };
  const probe = swap.probe;
  const noEmpty = probe.moSamples > 0 && probe.moMin >= 3 && probe.rafMin >= 3;
  return { pass: diffs.length === 0 && shellState.desktops === 1 && shellState.ver === '2' && shellState.desktopVer === '2' && noEmpty, summary: `Shell 1→2 (Doppelpuffer) mit 3 Fenstern (2× formular v2, 1× notizen v2), halbes Formular: stop ${data.swap.stopMs} ms + import ${data.swap.importMs} ms + start/umschalten ${data.swap.startMs} ms = ${data.swap.totalMs} ms; Abweichungen ${diffs.length}; nie leer: min. sichtbare Fenster mit Inhalt ${probe.moMin} über ${probe.moSamples} Mutationen, ${probe.rafMin} über ${probe.rafSamples} Frames; Remount nach Tausch ${data.remountAfterSwapMs} ms.`, data };
});

await scenario('f-zustandszone', async () => {
  const id = wA;
  const info = () => A.evaluate((i) => window.__lab.winInfo(i).zone, id);
  const empty = await info();
  await A.evaluate((i) => {
    window.__lab.report(i, { level: 'info', code: 'sync-hinweis', text: 'Hinweis der App' });
    window.__lab.report(i, { level: 'error', code: 'speichern', text: 'Speichern fehlgeschlagen: Feld „Name“ ist Pflicht und darf nicht leer sein, bitte ergänzen und erneut versuchen' });
    window.__lab.report(i, { level: 'warn', code: 'konflikt', text: 'Konflikt mit einer anderen Änderung' });
  }, id);
  const three = await info();
  await A.click(`[data-window-id="${id}"] .v4-status-zone__line`);
  const opened = await info();
  await A.screenshot({ path: path.join(HERE, 'shots/f-zone-liste-offen-tab-a.png') });
  const inside = opened.list.rect[0] >= opened.windowRect[0] - 0.5 && opened.list.rect[1] >= opened.windowRect[1] - 0.5 && opened.list.rect[2] <= opened.windowRect[2] + 0.5 && opened.list.rect[3] <= opened.windowRect[3] + 0.5;
  await A.evaluate((i) => { for (const c of ['sync-hinweis', 'speichern', 'konflikt']) window.__lab.clear(i, c); }, id);
  const cleared = await info();
  // Aktivierung > 2 s sichtbar, < 2 s still (gleiche Version: kein Remount, nur Zustandsfolge)
  const watch = await A.evaluate((i) => { const z = window.__lab.win(i).zone.el; window.__zoneFlips = []; new MutationObserver(() => window.__zoneFlips.push({ hidden: z.hidden, at: performance.now() })).observe(z, { attributes: true, attributeFilter: ['hidden'] }); return true; }, id);
  const cur = await A.evaluate((i) => window.__lab.win(i).ver, id);
  await A.evaluate((v) => window.__lab.publish('formular', v, { delays: { fetched: 1000 } }), cur);
  await A.waitForTimeout(1500);
  const shortFlips = await A.evaluate(() => window.__zoneFlips.length);
  await A.evaluate((v) => window.__lab.publish('formular', v, { delays: { fetched: 3000 } }), cur);
  await A.waitForTimeout(1700);
  const at1700 = await info();
  await A.waitForTimeout(700);
  const at2400 = await info();
  await A.waitForTimeout(1200);
  const afterActivation = await info();
  // Messwerte: Verbindung weg > 5 s → warn in allen Fenstern beider Tabs; Outbox > 10 s → warn
  await A.evaluate(() => { window.__lab.kernel.lab.setLink('down'); window.__lab.put({ id: `offline-${Date.now()}`, titel: 'offline geschrieben', prio: 1 }); });
  await A.waitForTimeout(4500);
  const link45 = await info();
  await A.waitForTimeout(1300);
  const link58A = await info();
  const link58B = await B.evaluate((ids) => ids.map((i) => window.__lab.winInfo(i).zone.text), [wB, wN, w3]);
  await A.waitForTimeout(5000);
  const outbox108 = await info();
  await A.evaluate(() => window.__lab.kernel.lab.setLink('up'));
  await A.waitForTimeout(1200);
  const afterUp = await info();
  const data = {
    empty: { hidden: empty.hidden, height: empty.height, visible: empty.visible },
    threeEntries: { height: r1(three.height), lineHeight: three.lineHeight, rowsInZone: three.rowsInZone, level: three.level, text: three.text.slice(0, 60), truncated: three.lineScrollWidth > three.lineClientWidth },
    listOpened: { hidden: opened.list.hidden, items: opened.list.items, insideWindow: inside, parentIsWindow: opened.list.parentIsWindow, rect: opened.list.rect.map(Math.round), windowRect: opened.windowRect.map(Math.round) },
    afterClear: { hidden: cleared.hidden, height: cleared.height, listHidden: cleared.list.hidden },
    activation: { shortDelay1sZoneFlips: shortFlips, at1700ms: { hidden: at1700.hidden }, at2400ms: { hidden: at2400.hidden, text: at2400.text }, afterActivated: { hidden: afterActivation.hidden } },
    metrics: { linkDown4500ms: { hidden: link45.hidden }, linkDown5800ms: { hidden: link58A.hidden, text: link58A.text }, tabBZonesAt5800ms: link58B, outboxAt10800ms: { entries: outbox108.entries.map((e) => e.code), text: outbox108.text }, afterLinkUp1200ms: { hidden: afterUp.hidden, entries: afterUp.entries.map((e) => e.code) } },
  };
  const pass = empty.hidden && empty.height === 0 && three.rowsInZone === 1 && three.level === 'error' && Math.abs(three.height - three.lineHeight - 1) <= 1
    && !opened.list.hidden && opened.list.items === 3 && inside && cleared.hidden && cleared.height === 0
    && shortFlips === 0 && at1700.hidden && !at2400.hidden && afterActivation.hidden
    && link45.hidden && !link58A.hidden && link58B.every((t) => t) && outbox108.entries.some((e) => e.code === 'metrics.outbox') && afterUp.hidden;
  return { pass, summary: `leer: hidden, ${empty.height} px; 3 Einträge: 1 Zeile (${r1(three.height)} px), schwerster = ${three.level}; Liste im Fenster: ${inside}; Aktivierung 1 s: unsichtbar (${shortFlips} Wechsel), 3 s: bei 1,7 s unsichtbar / 2,4 s sichtbar; Verbindung weg: 4,5 s unsichtbar, 5,8 s sichtbar in A und allen 3 Fenstern von B; Outbox > 10 s gemeldet; nach Wiederverbindung leer: ${afterUp.hidden}.`, data };
});

await scenario('h-zehn-fenster-und-css-reihenfolge', async () => {
  const ids = [];
  for (let i = 0; i < 7; i++) {
    const id = await B.evaluate((i) => window.__lab.openWindow('formular', { geometry: { x: 40 + i * 25, y: 60 + i * 20, w: 460, h: 360 } }), i);
    ids.push(id);
    await B.click(`[data-window-id="${id}"] #name`);
    await B.keyboard.type(`Fenster ${i}`);
  }
  const formWins = await B.evaluate(() => [...window.__lab.shell.host.windows.values()].filter((w) => w.appId === 'formular').map((w) => w.id));
  const total = await B.evaluate(() => window.__lab.shell.host.windows.size);
  // CSS-Reihenfolge beobachten: Hinzufügen neu → load neu → Entfernen alt
  await B.evaluate(() => {
    window.__cssLog = [];
    new MutationObserver((ms) => { for (const m of ms) { for (const n of m.addedNodes) if (n.dataset?.appCss !== undefined) { window.__cssLog.push(['add', n.getAttribute('href'), performance.now()]); n.addEventListener('load', () => window.__cssLog.push(['load', n.getAttribute('href'), performance.now()])); } for (const n of m.removedNodes) if (n.dataset?.appCss !== undefined) window.__cssLog.push(['remove', n.getAttribute('href'), performance.now()]); } }).observe(document.head, { childList: true });
  });
  const valuesBefore = await Promise.all(formWins.map((id) => readForm(B, id).then((f) => f.fields.name)));
  const allMs = [];
  let cur = await B.evaluate((id) => window.__lab.win(id).ver, formWins[0]);
  for (let r = 0; r < 6; r++) {
    const ver = cur === '1' ? '2' : '1';
    const t0 = await A.evaluate((v) => window.__lab.publish('formular', v), ver);
    const evs = await Promise.all(formWins.map((id) => waitState(B, { windowId: id, ver, state: 'mounted', t0 })));
    allMs.push(Math.max(...evs.map((e) => e.at)) - t0);
    cur = ver;
  }
  const valuesAfter = await Promise.all(formWins.map((id) => readForm(B, id).then((f) => f.fields.name)));
  const cssLog = await B.evaluate(() => window.__cssLog.map(([k, h, t]) => [k, h, Math.round(t)]));
  // Für den ersten Tausch: neue Datei muss vor dem Entfernen der alten geladen sein
  const firstRemove = cssLog.find((e) => e[0] === 'remove');
  const loadOfNew = firstRemove && cssLog.find((e) => e[0] === 'load' && e[1] !== firstRemove[1]);
  const orderOk = !!(firstRemove && loadOfNew && cssLog.indexOf(loadOfNew) < cssLog.indexOf(firstRemove));
  const heap = await B.evaluate(() => (performance.memory ? Math.round(performance.memory.usedJSHeapSize / 1048576) : null));
  const data = { windowsInTabB: total, formularWindows: formWins.length, publishToAllMountedMs: stats(allMs), raw: allMs.map(r1), valuesKept: JSON.stringify(valuesBefore) === JSON.stringify(valuesAfter), cssOrderNewLoadedBeforeOldRemoved: orderOk, cssLogFirst6: cssLog.slice(0, 6), appCssLinksAfter: await B.evaluate(() => document.querySelectorAll('link[data-app-css]').length), usedJsHeapMiB: heap };
  await B.screenshot({ path: path.join(HERE, 'shots/h-zehn-fenster-tab-b.png') });
  return { pass: data.valuesKept && orderOk && data.appCssLinksAfter === 2, summary: `${total} Fenster in Tab B (${formWins.length}× formular gleichzeitig): Veröffentlichen→alle mounted p50 ${data.publishToAllMountedMs.p50} ms / p95 ${data.publishToAllMountedMs.p95} ms (6 Wdh.); Werte erhalten: ${data.valuesKept}; CSS neu geladen vor Entfernen alt: ${orderOk}; danach ${data.appCssLinksAfter} App-CSS-Links.`, data };
});

await scenario('g-kein-reload-kein-datenabruf', async () => {
  const check = async (page, name) => page.evaluate(() => ({ sentinel: window.__lab.sentinel, navEntries: performance.getEntriesByType('navigation').map((e) => e.type) }));
  const sa = await check(A, 'A');
  const sb = await check(B, 'B');
  const byType = {};
  for (const r of requests) byType[r.type] = (byType[r.type] || 0) + 1;
  const allowed = (u) => u === '/?tab=A' || u === '/?tab=B' || /^\/lab\/[\w-]+\.js$/.test(u) || /^\/~a\/[\w-]+@[\w.-]+\/[\w./-]+\.(js|css)$/.test(u);
  const unexpected = requests.filter((r) => !allowed(r.url) || ['fetch', 'xhr', 'websocket', 'eventsource'].includes(r.type) || r.method !== 'GET');
  const resTiming = await B.evaluate(() => performance.getEntriesByType('resource').filter((e) => e.name.includes('/~a/')).map((e) => ({ url: new URL(e.name).pathname, startMs: Math.round(e.startTime), durationMs: Math.round(e.duration * 10) / 10 })));
  const data = { resourceTimingTabB: resTiming, requestsTotal: requests.length, byType, unexpected, distinctUrls: [...new Set(requests.map((r) => r.url))], mainFrameNavigations: navs, sentinelsUnchanged: sa.sentinel === sentinels.A && sb.sentinel === sentinels.B, navEntries: { A: sa.navEntries, B: sb.navEntries }, consoleErrors };
  const pass = unexpected.length === 0 && navs.A === 1 && navs.B === 1 && data.sentinelsUnchanged;
  return { pass, summary: `${requests.length} Anfragen gesamt (${Object.entries(byType).map(([k, v]) => `${k} ${v}`).join(', ')}), davon unerwartet/Daten: ${unexpected.length}; Hauptframe-Navigationen A=${navs.A}, B=${navs.B} (je 1 = nur der erste Aufruf); Seiten-Sentinel unverändert: ${data.sentinelsUnchanged}; Konsolenfehler: ${consoleErrors.length}.`, data };
});

await browser.close();
server.close();
results.finishedAt = new Date().toISOString();
await flush();
const failed = Object.entries(results.scenarios).filter(([, s]) => !s.pass).map(([n]) => n);
console.log(failed.length ? `NICHT BESTANDEN: ${failed.join(', ')}` : 'alle Szenarien bestanden');
process.exit(failed.length ? 1 : 0);
