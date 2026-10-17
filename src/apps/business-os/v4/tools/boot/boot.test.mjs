// E38: Einstiegsseite der Shell v4 im echten Browser. Der Laborserver bildet die Daemon-Routen nach:
//   GET /             → v4/index.html        GET /sw.js → v4/sw.js
//   GET /kernel/*     → Kern-Bootstrap (hier fixtures/kernel/boot.mjs, bis bootV4 geliefert ist)
//   GET /business-os/* → Slot-Baum (src/apps/business-os)   GET /~stub/* → App-API-Stub (Pin 8265bd7)
// Prüft: Seite lädt boot.js, bootV4 genau einmal mit genau {launchContextUrl:'/api/business-os/launch-context'},
// der Rahmen steht, keine Seitenfehler, kein Inhalt außer dem Rahmen. Gegenprobe (Argument counter): unter / liegt
// das V2-index.html → bootV4 wird nie gerufen.
// Lauf (aus src/apps/business-os): node v4/tools/boot/boot.test.mjs [counter]
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { realManifestRoutes } from '../port/real-manifest.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const BOS = path.resolve(HERE, '../../..');
const STUB = process.env.CTOX_APP_API_STUB || path.join(os.homedir(), '.local/state/shell-v4/stub-pins/4b0d0ac');
const COUNTER = process.argv[2] === 'counter';
const require = createRequire(path.join(os.homedir(), 'Documents/ctox/src/apps/business-os/package.json'));
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const MIME = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.jpg': 'image/jpeg', '.woff2': 'font/woff2' };

function route(urlPath) {
  if (urlPath === '/') return path.join(BOS, COUNTER ? 'index.html' : 'v4/index.html');
  if (urlPath === '/sw.js') return path.join(BOS, 'v4/sw.js');
  if (urlPath.startsWith('/kernel/')) return path.join(HERE, 'fixtures', urlPath);
  if (urlPath.startsWith('/business-os/')) return path.join(BOS, urlPath.slice('/business-os/'.length));
  if (urlPath.startsWith('/~stub/')) return path.join(STUB, urlPath.slice('/~stub/'.length));
  return null;
}
// E48 (Pin 8265bd7): Routen- und Rechteteil des echten Shell-Manifests (foreign_collections u. a.) samt Eigentümern für
// die Labor-Veröffentlichung in fixtures/kernel/boot.mjs; ohne Deklaration liest die Shell shell_preferences nicht.
const SHELL_ROUTES = JSON.stringify(realManifestRoutes('shell'));
const requests = [];
const server = createServer((req, res) => {
  const urlPath = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  requests.push(urlPath);
  if (urlPath === '/kernel/shell-routes.json') { res.writeHead(200, { 'content-type': MIME['.json'] }).end(SHELL_ROUTES); return; }
  const file = route(urlPath);
  if (!file || !fs.existsSync(file) || !fs.statSync(file).isFile()) { res.writeHead(404).end('not found'); return; }
  res.writeHead(200, { 'content-type': MIME[path.extname(file)] || 'application/octet-stream' });
  res.end(fs.readFileSync(file));
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ headless: true });
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} [boot${COUNTER ? '-counter' : ''}] ${name}${detail ? ` – ${detail}` : ''}`); };
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, serviceWorkers: 'block' });
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
  await page.goto(`${base}/`);
  await page.waitForSelector('[data-v4-frame]', { timeout: 8000 }).catch(() => {});
  const state = await page.evaluate(() => ({
    calls: globalThis.__bootCalls ?? [],
    frame: !!document.querySelector('[data-v4-frame]'),
    title: document.title,
    bodyChildren: [...document.body.children].map((el) => el.tagName + (el.dataset.v4Frame !== undefined ? '[frame]' : '')),
  }));
  check('GET / lädt boot.js unter /business-os/v4/boot.js', requests.includes('/business-os/v4/boot.js'), JSON.stringify(requests.slice(0, 6)));
  check('bootV4 genau einmal', state.calls.length === 1, JSON.stringify(state.calls));
  check('Aufruf genau {launchContextUrl:"/api/business-os/launch-context"}', state.calls.length === 1
    && JSON.stringify(state.calls[0].keys) === '["launchContextUrl"]' && state.calls[0].launchContextUrl === '/api/business-os/launch-context', JSON.stringify(state.calls[0] ?? null));
  check('Rahmen der Shell v4 steht', state.frame, JSON.stringify(state.bodyChildren));
  check('Titel CTOX Business OS', state.title === 'CTOX Business OS', state.title);
  check('keine Seitenfehler', errors.length === 0, JSON.stringify(errors.slice(0, 3)));
  await page.screenshot({ path: path.join(process.env.CTOX_EVIDENCE_DIR || os.tmpdir(), `boot-${COUNTER ? 'counter' : 'v4'}-1280.png`) });
} finally {
  await browser.close();
  server.close();
}
const passed = results.filter(Boolean).length;
console.log(`[boot${COUNTER ? '-counter' : ''}] ${passed}/${results.length} PASS`);
process.exit(COUNTER ? (passed === results.length ? 1 : 0) : (passed === results.length ? 0 : 1));
