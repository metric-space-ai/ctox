// Ladeweg-Labor Monaco (§16.50): node src/apps/business-os/v4/lab/ladeweg/monaco.mjs
// Öffnet die Code-Ansicht über den echten SW (monaco.html/monaco-lab.js) und prüft: Monaco geladen, Worker gleich-
// ursprünglich unter /~a/lib:monaco@0.0…/ gestartet und vom SW bedient (Lib-Misses aus dem Worker beantwortet das
// Kernel-Fenster), keine Warnung „falling back to main thread“, 0 Server-Treffer unter /~a/.
// MUT=sw-worker-own | editor-data liefert eine verdorbene sw.js bzw. editor.js (alter Stand) – dann muss es rot werden.
import { createRequire } from 'node:module';
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { startServer } from './server.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const V4 = path.resolve(here, '../..');
const BOS = path.resolve(V4, '..');
const require = createRequire(path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
const { chromium } = require('playwright');
const exe = path.join(os.homedir(), 'Library/Caches/ms-playwright/chromium_headless_shell-1223/chrome-headless-shell-mac-arm64/chrome-headless-shell');
const MUT = process.env.MUT || '';

// Abschluss der Shell-Dateien ab source-mode.js (relative Importe, new URL(…, import.meta.url), ctox:shared/, CSS).
function closure(start) {
  const seen = new Set();
  const todo = [start];
  const SPEC = /(?:\bfrom\s*|\bimport\s*\(\s*|\bimport\s*)(['"])([^'"\r\n]+)\1|new URL\(\s*(['"])([^'"\r\n]+)\3\s*,\s*import\.meta\.url|url\(\s*(['"]?)([^'")\s]+)\5\s*\)|@import\s+(['"])([^'"]+)\7/g;
  while (todo.length) {
    const rel = todo.pop();
    if (seen.has(rel) || !existsSync(path.join(BOS, rel))) continue;
    seen.add(rel);
    if (!/\.(m?js|css)$/.test(rel)) continue;
    for (const m of readFileSync(path.join(BOS, rel), 'utf8').matchAll(SPEC)) {
      const s = m[2] || m[4] || m[6] || m[8];
      if (!s) continue;
      if (s.startsWith('ctox:shared/')) todo.push(`shared/${s.slice(12)}`);
      else if (s.startsWith('./') || s.startsWith('../')) todo.push(path.posix.normalize(path.posix.join(path.posix.dirname(rel), s)));
    }
  }
  return [...seen].sort();
}

const shellFiles = closure('v4/source/source-mode.js');
const server = await startServer();
const browser = await chromium.launch({ executablePath: exe, headless: true });
const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
const page = await ctx.newPage();
const consoleLines = [];
const workers = [];
page.on('console', (m) => consoleLines.push(`${m.type()}: ${m.text()}`));
page.on('pageerror', (e) => consoleLines.push(`pageerror: ${e.message}`));
page.on('worker', (w) => { workers.push(w.url()); w.on('console', (m) => consoleLines.push(`worker ${m.type()}: ${m.text()}`)); });
if (MUT === 'sw-worker-own') {
  const src = readFileSync(path.join(V4, 'sw.js'), 'utf8');
  const from = "if (own && (own.type === 'window' || own.type === undefined)) return own;";
  if (!src.includes(from)) throw new Error('MUT sw-worker-own: Stelle fehlt');
  await ctx.route('**/sw.js*', (r) => r.fulfill({ status: 200, contentType: 'text/javascript', headers: { 'Service-Worker-Allowed': '/' }, body: src.replace(from, 'if (own) return own;') }));
}
if (MUT === 'editor-data') {
  const src = readFileSync(path.join(V4, 'source/editor.js'), 'utf8');
  const from = 'window.MonacoEnvironment = { getWorkerUrl: () => worker.href };';
  if (!src.includes(from)) throw new Error('MUT editor-data: Stelle fehlt');
  const old = "window.MonacoEnvironment = { getWorkerUrl: () => `data:text/javascript;charset=utf-8,${encodeURIComponent(`self.MonacoEnvironment={baseUrl:'${base}'};importScripts('${base}vs/base/worker/workerMain.js');`)}` };";
  await page.route('**/__spur4/v4/source/editor.js', (r) => r.fulfill({ status: 200, contentType: 'text/javascript', body: src.replace(from, old) }));
}
const t0 = Date.now();
await page.goto(`${server.url}/monaco.html?shellFiles=${Buffer.from(JSON.stringify(shellFiles)).toString('base64')}`);
await page.waitForFunction(() => document.documentElement.dataset.monReady, null, { timeout: 30000 });
const mon0 = await page.evaluate(() => window.__mon);
if (mon0.errors.length) console.log('Seitenfehler', mon0.errors);
await page.click('.v4-src__item[data-path="index.js"]').catch((e) => consoleLines.push(`click: ${e.message}`));
const edOk = await page.waitForSelector('.monaco-editor .view-line', { timeout: 20000 }).then(() => true, () => false);
// Monaco startet den Sprach-Worker (TypeScript für .js) kurz nach dem Modell; auf den Worker und seine Dateien warten.
await page.waitForFunction(() => window.__mon.misses.some((m) => m.includes('tsWorker.js')), null, { timeout: 25000 }).catch(() => {});
await page.waitForTimeout(2500);
const mon = await page.evaluate(() => ({ ...window.__mon, editor: document.querySelector('.v4-src')?.dataset.editor ?? null, controlled: !!navigator.serviceWorker.controller }));
mkdirSync(path.join(here, 'shots'), { recursive: true });
await page.screenshot({ path: path.join(here, 'shots', `monaco${MUT ? `-${MUT}` : ''}-1280.png`) });
const fallback = consoleLines.filter((l) => /falling back|could not create web worker/i.test(l));
const res = {
  at: new Date().toISOString(), mut: MUT || null, ms: Date.now() - t0, shellFiles: shellFiles.length,
  controlled: mon.controlled, root: mon.root, editor: mon.editor, editorRendered: edOk, workers,
  workerMisses: mon.misses.filter((m) => /worker/i.test(m)), misses: mon.misses.length, aborts: mon.aborts, hits: mon.hits,
  serverHitsUnderA: server.stats.artifactHits, serverPathsUnderA: server.stats.artifactPaths.slice(0, 10), fallback,
  errors: [...mon.errors, ...consoleLines.filter((l) => /^(error|pageerror|worker error)/.test(l))].slice(0, 20),
};
const ok = res.controlled && res.editor === 'monaco' && res.editorRendered && res.workers.length > 0
  && res.workers.every((u) => new URL(u).pathname.startsWith('/~a/lib:monaco@0.0') && new URL(u).origin === server.url)
  && res.workerMisses.some((m) => m.includes('tsWorker.js')) && res.fallback.length === 0 && res.serverHitsUnderA === 0 && res.errors.length === 0;
res.pass = ok;
writeFileSync(path.join(here, `results-monaco${MUT ? `-${MUT}` : ''}.json`), JSON.stringify(res, null, 2));
console.log(JSON.stringify(res, null, 2));
console.log(ok ? 'PASS Monaco mit Worker über den SW' : 'FAIL Monaco mit Worker über den SW');
await browser.close();
await server.stop();
process.exit(ok ? 0 : 1);
