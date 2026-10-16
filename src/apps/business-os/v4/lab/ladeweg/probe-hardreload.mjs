// Zusatzprobe: harter Neuladen (Shift+Reload / Page.reload ignoreCache) umgeht den Service Worker.
// Frage: Ist die Seite danach noch kontrolliert, und wohin gehen /~a/-Anfragen?
import { startServer } from './server.mjs';
const pw = await import('/Users/michaelwelsch/Documents/ctox/src/apps/business-os/node_modules/playwright/index.mjs');
const server = await startServer();
const browser = await pw.chromium.launch({ headless: true });
const context = await browser.newContext();
const page = await context.newPage();
await page.goto(`${server.url}/?mode=sw`);
await page.waitForFunction(() => document.documentElement.dataset.labReady === '1');
await page.evaluate(() => window.lab.init());
await page.evaluate(async () => {
  await window.lab.install({ app_id: 'shell', app_ver: 'v1', files: { 'shared/greet.js': { text: "export function greet(n){return 'Hallo '+n;}" } } });
  await window.lab.setActive('shell', 'v1');
  await window.lab.install({ app_id: 'p', app_ver: 'v1', files: { 'index.js': { text: "import {greet} from 'ctox:shared/greet.js'; export function mount(r){r.textContent=greet('p'); return {unmount(){}, probe(){return {}}};}" } } });
});
const before = await page.evaluate(() => ({ controlled: !!navigator.serviceWorker.controller, status: null }));
before.status = await page.evaluate(() => window.lab.status('/~a/p@v1/index.js'));
const normal = {};
await page.reload();
await page.waitForFunction(() => document.documentElement.dataset.labReady === '1');
normal.controlled = await page.evaluate(() => !!navigator.serviceWorker.controller);
normal.status = await page.evaluate(() => window.lab.status('/~a/p@v1/index.js'));
const cdp = await context.newCDPSession(page);
const hitsBefore = server.stats.artifactHits;
await cdp.send('Page.reload', { ignoreCache: true });
await page.waitForFunction(() => document.documentElement.dataset.labReady === '1');
const hard = {};
hard.controlled = await page.evaluate(() => !!navigator.serviceWorker.controller);
hard.registrationActive = await page.evaluate(async () => !!(await navigator.serviceWorker.getRegistration('/'))?.active);
hard.status = await page.evaluate(() => window.lab.status('/~a/p@v1/index.js'));
hard.serverHits = server.stats.artifactHits - hitsBefore;
// Folgt nach dem harten Neuladen ohne Eingriff eine Kontrolle? 3 s warten.
hard.controlledAfter3s = await page.evaluate(() => new Promise((res) => { if (navigator.serviceWorker.controller) return res(true); const t = setTimeout(() => res(false), 3000); navigator.serviceWorker.addEventListener('controllerchange', () => { clearTimeout(t); res(true); }, { once: true }); }));
// iframe im hart neu geladenen Dokument: neue Navigation, wird sie vom SW kontrolliert?
hard.iframeControlled = await page.evaluate(() => new Promise((res) => { const f = document.createElement('iframe'); f.src = '/?mode=sw&frame=1'; f.onload = () => res(!!f.contentWindow.navigator.serviceWorker.controller); document.body.append(f); }));
// Gegenmittel: Loader bittet den aktiven SW um claim().
hard.claim = await page.evaluate(() => new Promise(async (res) => {
  const t0 = performance.now();
  const reg = await navigator.serviceWorker.getRegistration('/');
  const t = setTimeout(() => res({ controlled: false }), 3000);
  navigator.serviceWorker.addEventListener('controllerchange', () => { clearTimeout(t); res({ controlled: true, ms: performance.now() - t0 }); }, { once: true });
  reg.active.postMessage('ctox:claim');
}));
hard.statusAfterClaim = await page.evaluate(() => window.lab.status('/~a/p@v1/index.js'));
hard.mountAfterClaim = await page.evaluate(async () => { try { const m = await import('/~a/p@v1/index.js'); const d = document.createElement('div'); m.mount(d); return d.textContent; } catch (e) { return 'FEHLER ' + e.message; } });
console.log(JSON.stringify({ browser: browser.version(), before, normal, hard }));
await browser.close();
await server.stop();
