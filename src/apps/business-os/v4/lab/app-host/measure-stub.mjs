// Shell v4 – Remount-Messung (a) und Rückfall (c) gegen den Kern-Stub 67229d2 (Standard in server.mjs, sonst
// CTOX_APP_API_STUB) über shell/kernel-adapter.js. Rückfall: mount-Frist 9 s, ≤ 10 s, kernel.activation.reportFailed
// setzt den Zeiger zurück, ein neues Fenster startet die gute Version. Aufruf: node src/apps/business-os/v4/lab/app-host/measure-stub.mjs [--reps 20]
// Ergebnis: results-stub.json und Abschnitt „Messwerte gegen den Kern-Stub“ in RESULTS.md (eigene Marken).
import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import os from 'node:os';
import { startServer } from './server.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const { chromium } = await import(path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright/index.mjs'));
const EXE = path.join(os.homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');
const REPS = Number(process.argv[process.argv.indexOf('--reps') + 1]) || 20;
const pct = (xs, p) => { const s = [...xs].sort((a, b) => a - b); return s[Math.min(s.length - 1, Math.ceil((p / 100) * s.length) - 1)]; };
const r1 = (x) => Math.round(x * 10) / 10;
const stats = (xs) => ({ n: xs.length, p50: r1(pct(xs, 50)), p95: r1(pct(xs, 95)), min: r1(Math.min(...xs)), max: r1(Math.max(...xs)) });
const files = async (app, ver) => ({
  'index.js': await readFile(path.join(HERE, 'apps', app, `v${ver}`, 'index.js'), 'utf8'),
  'index.css': await readFile(path.join(HERE, 'apps', app, `v${ver}`, 'index.css'), 'utf8'),
});

const out = { startedAt: new Date().toISOString(), reps: REPS, scenarios: {} };
async function flush() {
  await writeFile(path.join(HERE, 'results-stub.json'), JSON.stringify(out, null, 2));
  const md = path.join(HERE, 'RESULTS.md');
  let text = await readFile(md, 'utf8');
  const block = ['<!-- STUB:START -->', `_Automatisch von measure-stub.mjs, Lauf ${out.startedAt}._`, ''];
  for (const [n, s] of Object.entries(out.scenarios)) block.push(`### ${n} — ${s.pass ? 'BESTANDEN' : 'NICHT BESTANDEN'}`, '', s.summary, '', '```json', JSON.stringify(s.data, null, 1).slice(0, 3000), '```', '');
  block.push('<!-- STUB:END -->');
  if (text.includes('<!-- STUB:START -->')) text = text.replace(/<!-- STUB:START -->[\s\S]*<!-- STUB:END -->/, block.join('\n'));
  else text += `\n## Messwerte gegen den Kern-Stub\n\n${block.join('\n')}\n`;
  await writeFile(md, text);
}

const { server, url } = await startServer();
const browser = await chromium.launch({ executablePath: EXE, headless: true });
const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
const errors = [];
async function tab(name) {
  const p = await ctx.newPage();
  p.on('pageerror', (e) => errors.push(`${name}: ${e.message}`));
  p.on('console', (m) => { if (m.type() === 'error') errors.push(`${name}: ${m.text()}`); });
  await p.goto(`${url}/?tab=${name}&kernel=stub`);
  await p.waitForFunction(() => document.documentElement.dataset.labReady === '1', null, { timeout: 15000 });
  return p;
}
const waitEv = (page, q, timeout = 20000) => page.waitForFunction(({ windowId, ver, state, after }) =>
  window.__lab.events.find((e) => e.windowId === windowId && e.ver === ver && e.state === state && e.at >= after), q, { timeout, polling: 5 }).then((h) => h.jsonValue());
const readForm = (page, id) => page.evaluate((id) => {
  const root = window.__lab.win(id).el.querySelector('.v4-app-root');
  const a = document.activeElement;
  const f = {};
  for (const el of root.querySelectorAll('input, textarea, select')) f[el.id || el.name] = el.value;
  const sc = {};
  for (const el of root.querySelectorAll('[id]')) if (el.scrollHeight > el.clientHeight) sc[el.id] = el.scrollTop;
  return { ver: window.__lab.win(id).ver, f, focus: root.contains(a) ? a.id : null, sel: root.contains(a) ? [a.selectionStart, a.selectionEnd] : null, sc };
}, id);

try {
  const A = await tab('A');
  await A.waitForFunction(() => window.__lab.kernel.leader.get().isLeader, null, { timeout: 10000 });
  const B = await tab('B');
  for (const v of ['1']) {
    const r = await A.evaluate(({ f }) => window.__lab.stubPublish('formular', '1', f), { f: await files('formular', v) });
    out.initialPublish = r;
  }
  await B.waitForFunction(() => window.__lab.kernel.activation.current('formular').ver === '1', null, { timeout: 10000 });
  const wA = await A.evaluate(() => window.__lab.openWindow('formular', { geometry: { x: 20, y: 20, w: 560, h: 460 } }));
  const wB = await B.evaluate(() => window.__lab.openWindow('formular', { geometry: { x: 20, y: 20, w: 560, h: 460 } }));
  const st = await B.evaluate((id) => window.__lab.win(id).state, wB);
  if (st !== 'mounted') throw new Error(`Fenster B nicht mounted: ${st}`);
  const sel = (s) => `[data-window-id="${wB}"] ${s}`;
  await B.click(sel('#name'));
  await B.keyboard.type('Erika Mustermann');
  await B.selectOption(sel('#kategorie'), 'c');
  await B.click(sel('#beschreibung'));
  await B.keyboard.type(Array.from({ length: 25 }, (_, i) => `Zeile ${i}`).join('\n'));
  await B.evaluate((id) => { const el = window.__lab.win(id).el; el.querySelector('#beschreibung').setSelectionRange(30, 40); el.querySelector('#beschreibung').scrollTop = 100; el.querySelector('#liste').scrollTop = 0; }, wB);
  const pre = { 1: await files('formular', 1), 2: await files('formular', 2), 3: await files('formular', 3) };
  const tB = []; const tA = []; const tPub = []; const mism = [];
  for (let i = 0; i < REPS; i++) {
    const ver = String(i + 2); // jede Wiederholung eine neue Stub-Version (Inhalt abwechselnd v2/v1)
    const content = i % 2 === 0 ? pre[2] : pre[1];
    await B.keyboard.type(`·${i}`);
    await B.evaluate((id) => { window.__lab.win(id).el.querySelector('#beschreibung').scrollTop = 100; }, wB);
    const before = await readForm(B, wB);
    const t0 = await A.evaluate(() => window.__lab.now());
    const pubP = A.evaluate(({ v, f }) => window.__lab.stubPublish('formular', v, f), { v: ver, f: content });
    const [eb, ea, pub] = await Promise.all([waitEv(B, { windowId: wB, ver, state: 'mounted', after: t0 }), waitEv(A, { windowId: wA, ver, state: 'mounted', after: t0 }), pubP]);
    tB.push(eb.at - t0); tA.push(ea.at - t0); tPub.push(pub.t0 - t0);
    await B.waitForTimeout(30);
    const after = await readForm(B, wB);
    const d = [];
    for (const k of ['name', 'beschreibung', 'kategorie']) if (before.f[k] !== after.f[k]) d.push(k);
    if (before.focus !== after.focus) d.push('focus');
    if (JSON.stringify(before.sel) !== JSON.stringify(after.sel)) d.push('selection');
    if (before.sc.beschreibung !== after.sc.beschreibung) d.push(`scroll ${before.sc.beschreibung}→${after.sc.beschreibung}`);
    if (d.length) mism.push({ rep: i, ver, d });
  }
  out.scenarios['a-stub-remount-zwei-tabs'] = {
    pass: mism.length === 0,
    summary: `${REPS} Remounts gegen createStubKernel (latencyMs 20): Veröffentlichen (sim.publishAppVersion im Leiter A) → mounted in B p50 ${r1(pct(tB, 50))} ms / p95 ${r1(pct(tB, 95))} ms, in A p50 ${r1(pct(tA, 50))} / p95 ${r1(pct(tA, 95))} ms; Abweichungen Wert/Fokus/Auswahl/Scroll: ${mism.length}.`,
    data: { publishToMountedMs: { tabB_reader: stats(tB), tabA_leader: stats(tA) }, rawB: tB.map(r1), mismatches: mism, errors },
  };
  await flush();
  // (c) Rückfall: v3 (mount hängt) als neue Stub-Version
  await B.click(sel('#name'));
  const before = await readForm(B, wB);
  const curVer = before.ver;
  const t0 = await A.evaluate(() => window.__lab.now());
  await A.evaluate(({ f }) => window.__lab.stubPublishNoWait('formular', '99', f), { f: pre[3] });
  const eb = await waitEv(B, { windowId: wB, ver: curVer, state: 'rolled-back', after: t0 }, 20000);
  const after = await readForm(B, wB);
  const zone = await B.evaluate((id) => window.__lab.winInfo(id).zone, wB);
  // §16.19 Nr. 2: nach reportFailed steht der Zeiger in BEIDEN Tabs wieder auf der letzten guten Version.
  const pointerBack = (page) => page.waitForFunction((good) => {
    const a = window.__lab.stubKernel.activation.get('formular').get();
    return a && a.activated_ver === good && a.state === 'failed' && a.target_ver === '99';
  }, curVer, { timeout: 5000, polling: 20 }).then(() => true, () => false);
  const [backA, backB] = await Promise.all([pointerBack(A), pointerBack(B)]);
  const act = await B.evaluate(() => window.__lab.stubKernel.activation.get('formular').get());
  const appRow = await B.evaluate(() => window.__lab.stubKernel.apps.get().find((r) => r.id === 'formular'));
  const reports = [...await A.evaluate(() => window.__lab.kernel.failedReports), ...await B.evaluate(() => window.__lab.kernel.failedReports)];
  // Neues Fenster startet die gute Version (nicht die gemeldete 99).
  const wNew = await B.evaluate(() => window.__lab.openWindow('formular', { geometry: { x: 600, y: 40, w: 520, h: 420 } }));
  const newWin = await B.evaluate((id) => ({ ver: window.__lab.win(id).ver, state: window.__lab.win(id).state }), wNew);
  const wNewA = await A.evaluate(() => window.__lab.openWindow('formular', { geometry: { x: 600, y: 40, w: 520, h: 420 } }));
  const newWinA = await A.evaluate((id) => ({ ver: window.__lab.win(id).ver, state: window.__lab.win(id).state }), wNewA);
  await B.screenshot({ path: path.join(HERE, 'shots/c-stub-rueckfall-neues-fenster-tab-b.png') });
  const rollbackMs = r1(eb.at - t0);
  const inputsKept = JSON.stringify(before.f) === JSON.stringify(after.f);
  out.scenarios['c-stub-rueckfall'] = {
    pass: after.ver === curVer && zone.level === 'error' && inputsKept && rollbackMs <= 10000 && backA && backB
      && reports.some((r) => r.ver === '99' && r.delivered) && newWin.ver === curVer && newWin.state === 'mounted' && newWinA.ver === curVer && newWinA.state === 'mounted'
      && appRow?.activated_ver === curVer,
    summary: `v3-Inhalt als Stub-Version 99 → Rückfall auf ${curVer} nach ${rollbackMs} ms (≤ 10 s: ${rollbackMs <= 10000}); Zone: „${zone.text}“; Eingaben erhalten: ${inputsKept}; reportFailed zugestellt ${reports.filter((r) => r.delivered).length}×; Zeiger danach (A/B zurück: ${backA}/${backB}): ${JSON.stringify({ activated_ver: act?.activated_ver, target_ver: act?.target_ver, state: act?.state, error: act?.error })}; kernel.apps activated_ver ${appRow?.activated_ver}; neues Fenster B ${newWin.ver}/${newWin.state}, A ${newWinA.ver}/${newWinA.state}.`,
    data: { rollbackMs, zone: { level: zone.level, text: zone.text, entries: zone.entries }, activationAfter: act, pointerBack: { A: backA, B: backB }, reports, newWindow: { B: newWin, A: newWinA }, appRow: { activated_ver: appRow?.activated_ver, state: appRow?.state, layout: appRow?.layout, styles: appRow?.styles }, errors },
  };
  await flush();
} catch (err) {
  out.scenarios.abbruch = { pass: false, summary: `Abbruch: ${err.message}`, data: { stack: String(err.stack).split('\n').slice(0, 5), errors } };
  await flush();
}
await browser.close();
server.close();
for (const [n, s] of Object.entries(out.scenarios)) console.log(`[${n}] ${s.pass ? 'PASS' : 'FAIL'} ${s.summary}`);
